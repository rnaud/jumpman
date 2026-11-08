//! Port of Mario Kart 64's human-player driving model (n64decomp/mk64
//! `src/player_controller.c`, `src/data/kart_attributes.c`). Engine-free; `src/game/experiments/kartPhysics.ts`
//! is the line-for-line browser twin and both are checked against the same trace (see tests).
//!
//! Units are MK64's own: positions in course units, angles in s16 (65536 = 360 deg), one `step` = one
//! 60 Hz kart update. Callers convert to world units (x8) and run two steps per 30 Hz sim tick.
//! Decomp function names are kept in comments so the port can be diffed against the source.
//!
//! Ground contact follows the decomp: the floor reaction is `func_8003F46C` -> `func_8003E9EC` (Luigi Raceway),
//! the wall response is `func_8003F734` / `func_8003FBAC` + `func_8002C954` through `func_8002A5F4`, and the
//! slope terms come from the four tyre heights of `func_80029B4C` (`slopeAccel`, `unk_206`).
//!
//! Not ported (not reachable in free roam): items, spinouts, boost ramps, the Lakitu retrieval (the host does a
//! graceful rescue instead), stars, AB-spin, triple-A/B combos, CPU logic, the terrain tumble trigger, per-tyre
//! surface types other than grass/asphalt. The kart origin sits `BOUNDING_BOX` below the decomp's, so the
//! decomp's `surfaceDistance` is `normal.y * (y + BOUNDING_BOX - floor) - BOUNDING_BOX` here.

pub const FRAME_HZ: f64 = 60.;
/// gKartBoundingBoxSizeTable (Mario).
const BOUNDING_BOX: f64 = 5.5;
/// gCharacterSize (MARIO_SIZE) * 18: tyre spread. `+ 1` is the wheel base used for the slope angles.
const KART_LENGTH: f64 = 0.75 * 18.;
const WHEEL_BASE: f64 = KART_LENGTH + 1.;
const TYRE_RADIUS: f64 = 18. * (0.75 / 2.);
const FRICTION: f64 = 5800.; // gKartFrictionTable
const GRAVITY: f64 = 2600.; // gKartGravityTable
pub const TOP_SPEED: f64 = 320.; // gTopSpeed150cc (Mario)
const SPEED_CAP: f64 = 9.; // gKartTopSpeedTable
const UNK_084: f64 = -20.; // D_800E2400[150cc]
const UNK_088: f64 = 35.; // D_800E24B4[150cc]
const HANDLING: f64 = 1.25; // gKartHandlingTable (Mario)
const ACCEL: [f64; 10] = [2.0, 2.0, 2.0, 1.6, 1.4, 1.2, 1.0, 0.8, 0.6, 0.4]; // gKartAccelerationMario
const HOP_JERK: f64 = 0.03;
const HOP_VELOCITY: f64 = 0.93;
const DEG: i32 = 182; // DEGREES(1) as u16
const GRASS_BACK: f64 = 0.09; // D_800E2E90 (grass)
const GRASS_FRONT: f64 = 0.38; // D_800E2EB0 (grass)
const GRASS_BRAKE: f64 = 0.2; // D_800E3210 (grass)
const GRASS_SIDE_BACK: f64 = 0.58; // D_800E2A90 (grass)
const GRASS_SIDE_FRONT: f64 = 0.38; // D_800E2AB0 (grass)
const NON_ASPHALT_GRAVITY: [f64; 11] = [0.825, 0.8, 0.725, 0.625, 0.425, 0.3, 0.3, 0.3, 0.3, 0.3, 0.3];
// Generated from the decomp: sp44 (func_80033AE0), update_steering_large/small call rows.
// LARGE = (threshold, numerator<<12, unk_090 magnitude); SMALL = (threshold, numerator<<12, unk_090 step).
const SP44: [f64; 156] = [0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.6, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.5, 0.5, 0.5, 0.5, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.6, 0.6, 0.6, 0.6, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8];
const LARGE: [(i32, i32, f64); 59] = [(90, 120, 450.), (89, 118, 440.), (88, 116, 430.), (87, 114, 420.), (86, 112, 410.), (85, 88, 400.), (84, 86, 395.), (83, 80, 390.), (82, 79, 390.), (81, 78, 380.), (80, 77, 370.), (79, 76, 360.), (78, 76, 360.), (77, 75, 350.), (76, 74, 340.), (75, 73, 330.), (74, 73, 330.), (73, 73, 330.), (72, 72, 320.), (71, 71, 315.), (70, 71, 315.), (69, 70, 305.), (68, 70, 305.), (67, 69, 280.), (66, 70, 270.), (65, 69, 270.), (64, 68, 260.), (63, 67, 250.), (62, 67, 250.), (61, 67, 250.), (60, 61, 245.), (59, 60, 245.), (58, 59, 245.), (57, 58, 245.), (56, 56, 245.), (55, 56, 230.), (54, 56, 230.), (53, 56, 230.), (52, 56, 230.), (51, 56, 230.), (50, 50, 220.), (49, 50, 220.), (48, 50, 220.), (47, 50, 220.), (46, 50, 220.), (45, 48, 110.), (44, 46, 110.), (43, 46, 110.), (42, 46, 110.), (41, 46, 110.), (40, 46, 110.), (39, 44, 110.), (38, 40, 110.), (37, 40, 110.), (36, 36, 110.), (35, 36, 110.), (34, 34, 110.), (32, 32, 100.), (31, 32, 100.)];
const SMALL: [(i32, i32, f64); 31] = [(30, 31, 0.9), (29, 30, 0.9), (28, 29, 0.9), (27, 28, 0.9), (26, 27, 0.9), (25, 26, 1.0), (24, 25, 1.0), (23, 24, 1.0), (22, 23, 1.0), (21, 22, 1.0), (20, 21, 1.05), (19, 20, 1.05), (18, 19, 1.05), (17, 18, 1.05), (16, 17, 1.05), (15, 16, 1.2), (14, 15, 1.2), (13, 14, 1.2), (12, 13, 1.2), (11, 12, 1.2), (10, 14, 1.6), (9, 13, 1.6), (8, 12, 1.6), (7, 11, 1.6), (6, 10, 1.6), (5, 9, 1.9), (4, 8, 1.9), (3, 7, 1.9), (2, 6, 1.9), (1, 5, 1.9), (0, 0, 1.9)];

#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// N64 stick units (about -80..80).
    pub raw_x: i32,
    pub raw_y: i32,
    /// A button (accelerate).
    pub accel: bool,
    /// B button (brake / reverse).
    pub brake: bool,
    /// R button held (hop and drift).
    pub hop_down: bool,
    /// R button went down this frame.
    pub hop_pressed: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Floor {
    pub height: f64,
    /// Upward unit normal.
    pub normal: [f64; 3],
    pub grass: bool,
}

/// One wall the kart overlaps.
#[derive(Clone, Copy, Debug)]
pub struct WallHit {
    /// Unit normal pointing out of the wall, towards the kart.
    pub normal: [f64; 3],
    /// Mostly faces the X axis (decomp `unk54` / `func_8003FBAC`); otherwise the Z axis (`unk48` / `func_8003F734`).
    pub x_facing: bool,
}

#[derive(Clone, Debug)]
pub struct Wall {
    /// The kart position pushed out of every wall.
    pub pos: [f64; 3],
    pub hits: Vec<WallHit>,
}

pub trait Ground {
    /// Highest floor at/below `y` (course units).
    fn floor(&self, x: f64, y: f64, z: f64) -> Option<Floor>;
    /// If the kart at `pos` overlaps a wall, the corrected position and the walls hit.
    fn wall(&self, pos: [f64; 3]) -> Option<Wall>;
}

#[derive(Clone, Debug)]
pub struct Kart {
    pub pos: [f64; 3],
    old_pos: [f64; 3],
    pub velocity: [f64; 3],
    /// rotation[1], s16 units; forward is (-sin a, cos a).
    pub rotation: i32,
    pub current_speed: f64,
    pub speed: f64,
    previous_speed: f64,
    unk_08c: f64,
    unk_0e8: f64,
    unk_0a0: f64,
    unk_104: f64,
    pub unk_078: i32,
    unk_090: f64,
    unk_0c0: i32,
    unk_20c: f64,
    unk_208: f64,
    pub steer_position: i32,
    steer_change_increment: u32,
    steer_position_delta: i32,
    pub hop: bool,
    pub drifting: bool,
    pub midair: bool,
    pub braking: bool,
    pub mini_turbo: bool,
    pub drift_outside: bool,
    pub drift_state: i32,
    pub drift_state_counter: i32,
    drift_duration: i32,
    unk_23a: i32,
    hop_velocity: f64,
    hop_accel: f64,
    hop_jerk: f64,
    kart_gravity: f64,
    unk_dac: f64,
    pub back_up: bool,
    move_backwards: bool,
    left_turn: bool,
    right_turn: bool,
    turn_frames: i32,
    normal: [f64; 3],
    pub surface_distance: f64,
    pub grass: bool,
    air_frames: i32,
    pub bumped: bool,
    /// Wall hits since the kart was created (the camera and sound watch this between 30 Hz ticks).
    pub bumps: u32,
    /// `slopeAccel` (s16): pitch of the floor under the tyres, positive downhill.
    pub slope_accel: i32,
    /// `unk_206` (s16): roll of the floor across the tyres.
    roll_slope: i32,
    /// Smoothed tyre heights: left, right, front, back (`unk_230`, `unk_23C`, `unk_1FC`, `unk_1F8`).
    tyre_heights: [f64; 4],
    tyres_ready: bool,
    /// Grass under each tyre: front left, front right, back left, back right.
    tyre_grass: [bool; 4],
    /// `unk_064`: last frame's ground reaction / brake-hold force.
    ground_force: [f64; 3],
}

fn s16(v: f64) -> i32 {
    (v as i32) as i16 as i32
}
fn sin_a(a: i32) -> f64 {
    (a as f64 * std::f64::consts::PI / 32768.).sin()
}
fn cos_a(a: i32) -> f64 {
    (a as f64 * std::f64::consts::PI / 32768.).cos()
}
fn atan2s(y: f64, x: f64) -> i32 {
    (y.atan2(x) * 32768. / std::f64::consts::PI) as i32
}
fn towards(value: &mut f64, target: f64, rate: f64) {
    // move_f32_towards (snaps tiny values to zero)
    *value = (1. - rate) * *value + rate * target;
    if value.abs() < 0.001 {
        *value = 0.;
    }
}
/// move_s16_towards
fn towards_s16(value: i32, target: i32, rate: f64) -> i32 {
    s16(value as f64 - (value - target) as f64 * rate)
}
/// `func_8002A5F4`: reflect the part of `v` along `n` with `restitution` when `distance < -threshold`,
/// otherwise just slide along the surface.
fn reflect(n: [f64; 3], distance: f64, v: &mut [f64; 3], restitution: f64, threshold: f64) {
    let dot = n[0] * v[0] + n[1] * v[1] + n[2] * v[2];
    let k = if distance < -threshold { 1. + restitution } else { 1. };
    for i in 0..3 {
        v[i] -= dot * n[i] * k;
    }
}

impl Kart {
    pub fn new(pos: [f64; 3], rotation: i32) -> Self {
        Self {
            pos,
            old_pos: pos,
            velocity: [0.; 3],
            rotation,
            current_speed: 0.,
            speed: 0.,
            previous_speed: 0.,
            unk_08c: 0.,
            unk_0e8: 0.,
            unk_0a0: 0.,
            unk_104: 0.,
            unk_078: 0,
            unk_090: 0.,
            unk_0c0: 0,
            unk_20c: 0.,
            unk_208: UNK_088,
            steer_position: 0,
            steer_change_increment: 0,
            steer_position_delta: 0,
            hop: false,
            drifting: false,
            midair: false,
            braking: false,
            mini_turbo: false,
            drift_outside: false,
            drift_state: 0,
            drift_state_counter: 0,
            drift_duration: 0,
            unk_23a: 0,
            hop_velocity: 0.,
            hop_accel: 0.,
            hop_jerk: 0.,
            kart_gravity: GRAVITY,
            unk_dac: 1.,
            back_up: false,
            move_backwards: false,
            left_turn: false,
            right_turn: false,
            turn_frames: 0,
            normal: [0., 1., 0.],
            surface_distance: 0.,
            grass: false,
            air_frames: 0,
            bumped: false,
            bumps: 0,
            slope_accel: 0,
            roll_slope: 0,
            tyre_heights: [0.; 4],
            tyres_ready: false,
            tyre_grass: [false; 4],
            ground_force: [0.; 3],
        }
    }
    /// speed / 18 * 216
    pub fn kmh(&self) -> f64 {
        self.speed / 18. * 216.
    }
    /// Heading in radians using the browser convention: forward = (sin y, cos y).
    pub fn yaw(&self) -> f64 {
        -(self.rotation as f64) * std::f64::consts::PI / 32768.
    }
    pub fn slip_degrees(&self) -> i32 {
        self.unk_0c0 / DEG
    }
    fn slope_degrees(&self) -> i32 {
        // slopeAccel / DEGREES(1): positive downhill along the heading.
        self.slope_accel / DEG
    }

    /// `kart_hop` + `func_80036C5C`
    fn kart_hop(&mut self) {
        self.hop_jerk = HOP_JERK;
        self.hop_accel = 0.;
        self.hop_velocity = HOP_VELOCITY;
        self.hop = true;
        self.unk_dac = 3.;
        self.kart_gravity = 500.;
        if self.kmh() > 20. {
            self.drift_duration = 0;
            self.drifting = true;
        }
    }

    /// `cancel_drift_effect`
    fn cancel_drift(&mut self) {
        if self.drifting {
            if self.unk_0c0 / DEG > 0 {
                self.steer_position = (((self.steer_position >> 16) * 13 + 13 * 53) / (2 * 53) + 40) << 16;
            }
            if self.unk_0c0 / DEG < 0 {
                self.steer_position = (((self.steer_position >> 16) * 13 + 13 * 53) / (2 * 53) - 53) << 16;
            }
            self.drifting = false;
        }
    }

    fn clamped_stick_x(raw_x: i32, raw_y: i32) -> i32 {
        // get_clamped_stickX_with_deadzone
        let (mut x, mut y) = (raw_x, raw_y);
        let (mut x2, mut y2) = (x, y);
        if x > 12 {
            y2 = y * 12 / x;
            x2 = 12;
        }
        if x2 < -12 {
            y2 = y2 * 12 / -x2;
            x2 = -12;
        }
        if y2 > 12 {
            x2 = x2 * 12 / y2;
            y2 = 12;
        }
        if y2 < -12 {
            x2 = x2 * 12 / -y2;
            y2 = -12;
        }
        if raw_x > -13 && raw_x < 13 && raw_y > -13 && raw_y < 13 {
            x = 0;
            y = 0;
        } else {
            x -= x2;
            y -= y2;
        }
        if x > 0x35 {
            y = y * 0x35 / x;
            x = 0x35;
        }
        if x < -0x35 {
            y = y * 0x35 / -x;
            x = -0x35;
        }
        let _ = y;
        x
    }

    fn steer_inc(&mut self, minimum: i32) {
        let before = self.steer_change_increment;
        self.steer_change_increment = self.steer_change_increment.wrapping_sub(1 << 11);
        if self.steer_change_increment >= 0xF000_0000 {
            self.steer_change_increment = before;
        }
        if minimum >= self.steer_change_increment as i32 {
            self.steer_change_increment = minimum as u32;
        }
    }

    /// `func_80033AE0`: steering and the yaw rate (`unk_078`).
    fn steering(&mut self, input: &Input) {
        let kmh = self.kmh();
        if (!self.hop && (self.unk_0c0 / DEG <= 6 && self.unk_0c0 / DEG >= -6 || !input.hop_down)) || kmh <= 20. {
            self.cancel_drift();
        }
        if self.unk_0c0 / DEG < -5 {
            self.left_turn = true;
            self.right_turn = false;
            self.turn_frames += 1;
        } else if self.unk_0c0 / DEG > 5 {
            self.right_turn = true;
            self.left_turn = false;
            self.turn_frames += 1;
        } else {
            self.left_turn = false;
            self.right_turn = false;
            self.turn_frames = 0;
        }
        if self.hop || self.drifting {
            self.left_turn = false;
            self.right_turn = false;
        }
        let mut steer_position = self.steer_position;
        let mut clamped_x = Self::clamped_stick_x(input.raw_x, input.raw_y);
        if self.back_up || self.move_backwards {
            clamped_x = -clamped_x;
        }
        self.steer_position = ((clamped_x as i64) << 16) as i32;
        let delta = (steer_position.wrapping_sub(self.steer_position)) >> 16;
        self.steer_position_delta = delta as i16 as i32;
        let a0 = if self.drifting && !self.hop { 3 } else { 0 };
        let (large, small);
        if kmh >= 15. {
            let straight = if self.right_turn {
                delta <= 35 && delta >= 0
            } else if self.left_turn {
                delta >= -35 && delta <= 0
            } else {
                false
            };
            if self.right_turn || self.left_turn {
                if straight {
                    large = (a0 + 15) as i32;
                    small = (a0 + 15) as i32;
                } else {
                    large = a0 + 5;
                    small = a0 + 9;
                }
            } else {
                large = a0 + 3;
                small = a0 + 6;
            }
        } else {
            large = 8;
            small = 8;
        }
        for &(thr, num, arg6) in LARGE.iter() {
            if delta >= thr || -thr >= delta {
                self.steer_inc((num << 12) / large);
                steer_position = if self.steer_position < steer_position {
                    steer_position.wrapping_sub(self.steer_change_increment as i32)
                } else {
                    steer_position.wrapping_add(self.steer_change_increment as i32)
                };
                if self.unk_090 < arg6 {
                    self.unk_090 = -arg6;
                }
            }
        }
        for &(thr, num, step) in SMALL.iter() {
            if delta >= thr || -thr >= delta {
                self.steer_inc((num << 12) / small);
                steer_position = if self.steer_position < steer_position {
                    steer_position.wrapping_sub(self.steer_change_increment as i32)
                } else {
                    steer_position.wrapping_add(self.steer_change_increment as i32)
                };
                self.unk_090 += step;
                if self.unk_090 >= 0. {
                    self.unk_090 = 0.;
                }
            }
        }
        let sp = steer_position >> 16;
        let mut f2 = if self.drifting {
            (sp / 8) as f64
        } else if kmh <= 25. {
            (sp / 12) as f64
        } else {
            sp as f64 / (8. + self.current_speed / 50.)
        };
        f2 = f2.abs();
        let table = SP44[(kmh as i32).clamp(0, 155) as usize];
        f2 *= if self.drifting { table } else { table * 1.5 };
        self.steer_position = steer_position;
        self.drift_outside = false;
        let var_f12 = 0.; // D_800E3410 is all zeros
        let sp = steer_position >> 16;
        if !self.hop && !self.drifting {
            let handling = if sp >= 45 || sp <= -45 { 0.15 + HANDLING } else { HANDLING };
            if !self.braking {
                self.unk_078 = s16(sp as f64 * (f2 + f2 * var_f12) * handling);
            } else if kmh < 8. {
                self.unk_078 = s16(sp as f64 * (f2 + f2 * var_f12));
            } else if kmh < 65. {
                self.unk_078 = s16(sp as f64 * ((f2 + 1.5) + f2 * var_f12));
            } else {
                self.unk_078 = s16(sp as f64 * ((f2 + 1.6) + f2 * var_f12));
            }
            self.drift_state_counter = 0;
            if self.drift_state < 2 {
                self.drift_state = 0;
            }
        } else if !self.midair && !self.hop {
            let var_s1;
            if self.unk_0c0 / DEG > 0 {
                var_s1 = (sp * 13 + 13 * 53) / (2 * 53) + 40;
                if sp <= -40 {
                    self.drift_outside = true;
                }
            } else {
                var_s1 = (sp * 13 + 13 * 53) / (2 * 53) - 53;
                if sp >= 40 {
                    self.drift_outside = true;
                }
            }
            self.update_drift_state_counter(sp);
            let v = var_s1 as f64;
            self.unk_078 = if kmh < 8. {
                s16(v * ((f2 + 2.) + f2 * var_f12))
            } else if kmh < 65. {
                s16(v * ((f2 + 3.) + f2 * var_f12))
            } else {
                s16(v * ((f2 + 3.5) + f2 * var_f12))
            };
            self.unk_078 = s16(self.unk_078 as f64 * if self.drift_outside { 0.9 } else { 0.65 });
        } else {
            let mut v = sp;
            if clamped_x == 0 {
                v = 0;
            }
            self.unk_078 = if kmh <= 5. { s16(v as f64 * (f2 + 6.)) } else { s16(v as f64 * (f2 + 1.5)) };
        }
    }

    /// `update_drift_state_counter`
    fn update_drift_state_counter(&mut self, sp: i32) {
        let against = if self.unk_0c0 / DEG > 0 { sp <= -10 } else { sp >= 10 };
        if against {
            if self.drift_state_counter <= 100 {
                self.drift_state_counter += 1;
            }
        } else {
            if self.drift_state_counter >= 18 && self.drift_state_counter < 100 && self.drift_state < 3 {
                self.drift_state += 1;
            }
            if self.drift_state_counter >= 10 && self.drift_state_counter < 100 {
                self.drift_state_counter = 10;
            } else {
                self.drift_state_counter = 0;
                self.drift_state = 0;
            }
        }
    }

    fn decelerate(&mut self, amount: f64) {
        // player_decelerate_alternative
        self.current_speed -= amount;
        if self.current_speed <= 0. {
            self.current_speed = 0.;
        }
        if self.speed < 0.2 {
            self.unk_08c = 0.;
        }
        if TOP_SPEED <= self.current_speed {
            self.current_speed = TOP_SPEED;
        }
        if !self.midair {
            self.unk_08c = self.current_speed * self.current_speed / 25.;
        }
    }

    fn accelerate(&mut self) {
        // player_accelerate_alternative (no triple-A combo)
        let slope = self.slope_degrees() as f64;
        let tier = ((self.current_speed / (TOP_SPEED * 0.1)).floor() as i32).clamp(0, 9) as usize;
        if self.current_speed >= 0. && self.current_speed <= TOP_SPEED {
            self.current_speed += ACCEL[tier] + 0.05 * slope;
        }
        if self.current_speed < 0. {
            self.current_speed = 0.;
        }
        if TOP_SPEED <= self.current_speed {
            self.current_speed = TOP_SPEED;
        }
        if !self.midair {
            self.unk_08c = self.current_speed * self.current_speed / 25.;
        }
    }

    /// `func_800323E4`
    fn brake(&mut self) {
        self.braking = true;
        let var = if self.grass { GRASS_BRAKE * 2. } else { 0. };
        if self.previous_speed - self.speed <= 0. {
            self.unk_20c = 0.;
        } else {
            self.unk_20c = (self.unk_20c + 0.02).min(2.);
        }
        if self.kmh() <= 20. {
            self.decelerate((1. - var) * 4.);
        }
        if self.unk_20c >= 2. {
            self.decelerate((1. - var) * 2.5);
        } else {
            self.decelerate((1. - var) * 1.2);
        }
    }

    /// `func_80037CFC` (input half of a frame).
    fn handle_input(&mut self, input: &Input) {
        if !self.midair && !self.hop && !self.drifting && input.hop_pressed {
            self.kart_hop();
        }
        if !self.midair || (self.hop && self.surface_distance <= 5.) {
            self.steering(input);
        }
        self.braking = false;
        if !self.back_up {
            if input.accel {
                self.accelerate();
            } else {
                self.decelerate(1.);
            }
            if input.brake {
                self.brake();
            } else {
                self.unk_20c = 0.;
            }
        }
        if input.raw_y < -0x31 && self.kmh() <= 5. && input.brake {
            self.current_speed = 140.;
            self.back_up = true;
            self.unk_08c = self.current_speed * self.current_speed / 25.;
            self.unk_20c = 0.;
        }
        if (input.raw_y >= -0x1D || !input.brake) && self.back_up {
            self.back_up = false;
            self.current_speed = 0.;
        }
    }

    /// `func_8002A79C`
    fn mini_turbo_tick(&mut self) {
        if !self.mini_turbo && !self.drifting && self.drift_state >= 2 {
            self.mini_turbo = true;
            self.unk_23a = 0;
            self.drift_state = 0;
            self.drift_state_counter = 0;
        } else if self.mini_turbo {
            self.unk_23a += 1;
            if self.unk_23a >= 0x1F {
                self.unk_23a = 0;
                self.mini_turbo = false;
                self.drift_state = 0;
                self.drift_state_counter = 0;
            }
        }
    }

    /// `func_8002AB70`
    fn gravity_tick(&mut self) {
        let slope = self.slope_degrees();
        if !self.midair && self.unk_08c > 0. {
            if slope < -1 && slope >= -0x14 && self.kmh() >= 20. {
                towards(&mut self.kart_gravity, 500., 1.);
                towards(&mut self.unk_dac, 3., 0.05);
            } else {
                towards(&mut self.kart_gravity, GRAVITY, 0.1);
                towards(&mut self.unk_dac, 1., 0.07);
            }
        } else {
            if self.surface_distance >= 50. {
                self.unk_dac = 2.;
            }
            towards(&mut self.kart_gravity, GRAVITY, 0.02);
            towards(&mut self.unk_dac, 1., 0.07);
        }
    }

    /// `func_80036DB4` / `func_800371F4`: sideways + braking force from steering.
    fn lateral(&self) -> [f64; 3] {
        if self.unk_078 == 0 {
            return [0.; 3];
        }
        let m = if self.unk_078 < 0 { 1. } else { -1. };
        let kmh = self.kmh();
        let (var_f18, sp20);
        if self.drifting && !self.hop {
            var_f18 = self.unk_208 + (-kmh * 3.) + (-self.unk_20c * 50.);
            sp20 = UNK_084 * 3.;
        } else {
            let mut d = self.steer_position_delta;
            if d > 0 {
                d = -d;
            }
            let s = (self.steer_position >> 16) as f64;
            let base = self.unk_208 + (-kmh / 3.);
            var_f18 = if (-20..=20).contains(&(self.steer_position >> 16)) {
                if d < 20 {
                    base + (-self.current_speed * 0.02) + (-self.unk_20c * 50.)
                } else {
                    base + (m * s * 0.01 + (-self.current_speed * 0.05)) + (-self.unk_20c * 50.)
                }
            } else {
                base + (m * s * 0.1 + (-self.current_speed * 0.15)) + (-self.unk_20c * 50.)
            };
            sp20 = UNK_084;
        }
        let local = [m * (self.unk_090 + var_f18) * self.speed, 0., self.speed * sp20];
        self.to_world(local, self.rotation)
    }

    fn to_world(&self, v: [f64; 3], a: i32) -> [f64; 3] {
        let (s, c) = (sin_a(a), cos_a(a));
        [v[0] * c - v[2] * s, v[1], v[0] * s + v[2] * c]
    }

    /// `func_80030150`: forward thrust.
    fn thrust(&mut self) -> f64 {
        let mut var = 0.;
        let kmh = self.kmh();
        if kmh >= 8. {
            if self.grass {
                var += GRASS_BACK * 2. + GRASS_FRONT + GRASS_BACK;
            }
            if !self.midair {
                let temp_lo = self.slope_degrees() as f64;
                if kmh >= 20. {
                    if temp_lo > 17. || temp_lo < -17. {
                        var -= temp_lo * 0.0126 / 3.;
                    } else {
                        var -= temp_lo * 0.026 / 3.;
                    }
                } else {
                    var += -0.2;
                }
                let slip = (self.unk_0c0 / DEG).abs() as f64;
                if self.drifting || self.drift_duration > 0 {
                    var += slip * 0.004;
                } else {
                    var += slip * 0.01;
                }
                if self.drift_outside && self.drift_state_counter < 10 {
                    var += slip * 0.008;
                }
            }
            if self.midair {
                let target = self.unk_08c * 0.04;
                towards(&mut self.unk_0a0, target, 0.15);
            } else {
                towards(&mut self.unk_0a0, 0., 0.1);
            }
        } else {
            self.unk_0a0 = 0.;
            self.unk_0e8 = 0.;
            if self.slope_degrees() < 0 {
                var += -0.85;
            }
        }
        if self.mini_turbo && !self.drifting {
            towards(&mut self.unk_0e8, 580., 0.2);
        } else {
            towards(&mut self.unk_0e8, 0., 0.01);
        }
        towards(&mut self.unk_104, var, 0.05);
        let mut total = self.unk_08c + self.unk_0e8 - self.unk_0a0;
        if total < 0. {
            total = 0.;
        }
        (1. - self.unk_104) * total
    }

    /// `func_8002FCA8`
    fn side_grip_tick(&mut self) {
        let mut v = 0.;
        if self.grass {
            v += GRASS_SIDE_BACK * 2. + GRASS_SIDE_FRONT * 2.;
        }
        if self.current_speed >= 200. && self.turn_frames & 4 != 0 {
            let t = self.unk_0c0 / DEG;
            if t > 0xF || t < -0xF {
                v += 1.;
            }
        }
        let f12 = if UNK_088 >= 0. { UNK_088 * v } else { -UNK_088 * v };
        self.unk_208 = UNK_088 - f12;
    }

    /// `func_8002AE38`: slip angle between heading and travel.
    fn slip_tick(&mut self, old: [f64; 3], next: [f64; 3]) {
        let sp28 = sin_a(-self.rotation) * self.speed + old[0];
        let t16 = cos_a(-self.rotation) * self.speed + old[2];
        let prev = self.unk_0c0;
        if !self.drifting && (self.kmh() <= 8. || ((self.steer_position >> 16) < 5 && (self.steer_position >> 16) > -5)) {
            self.unk_0c0 = self.unk_078 * 9;
            let t = s16((self.unk_0c0 - prev) as f64);
            self.unk_0c0 = prev + t / 15;
        } else {
            let var_v1 = if self.turn_frames & 8 != 0 { 2 } else { 0 };
            let diff = s16(
                (atan2s(old[0] - next[0], old[2] - next[2]) - atan2s(old[0] - sp28, old[2] - t16)) as f64,
            );
            if self.current_speed >= 200. && var_v1 == 2 && (self.unk_0c0 / DEG >= 0x10 || self.unk_0c0 / DEG < -0xF) {
                self.unk_0c0 = diff;
            } else {
                self.unk_0c0 = diff * 2;
            }
            let sp = self.steer_position >> 16;
            if !self.drifting && ((sp > 0 && self.unk_0c0 < 0) || (sp < 0 && self.unk_0c0 > 0)) {
                self.unk_0c0 = self.unk_078 * 0x14;
                let t = s16((self.unk_0c0 - prev) as f64);
                self.unk_0c0 = prev + t / 12;
            } else {
                self.unk_0c0 = self.unk_0c0.clamp(-0x1C70, 0x1C70);
                let t = s16((self.unk_0c0 - prev) as f64);
                self.unk_0c0 = prev + t / 12;
            }
        }
    }

    /// `func_80029B4C`: floor height under each tyre -> `slopeAccel` and `unk_206`. Uses the position the
    /// kart had at the start of this update and `next_y` for the airborne case.
    fn sample_tyres(&mut self, ground: &dyn Ground, next_y: f64) {
        let r = TYRE_RADIUS;
        // front left, front right, back left, back right (x, z) in kart space.
        let offsets = [(r - 3.6, r - 2.), (-r + 3.6, r - 2.), (r - 2.6, -r + 4.), (-r + 2.6, -r + 4.)];
        let angle = self.rotation + self.unk_0c0;
        let mut base = [0.; 4];
        for (i, (x, z)) in offsets.iter().enumerate() {
            let w = self.to_world([*x, 0., *z], angle);
            let (tx, tz) = (self.pos[0] + w[0], self.pos[2] + w[2]);
            // A tyre touches when the floor is within one bounding box of the kart origin.
            let touching = ground
                .floor(tx, self.pos[1] + BOUNDING_BOX + 1., tz)
                .filter(|f| f.height <= self.pos[1] + BOUNDING_BOX && f.height >= self.pos[1] - BOUNDING_BOX);
            base[i] = match touching {
                Some(f) => {
                    self.tyre_grass[i] = f.grass;
                    f.height
                }
                None => self.pos[1],
            };
        }
        if !self.midair {
            let target = [
                (base[2] + base[0]) / 2., // left
                (base[3] + base[1]) / 2., // right
                (base[1] + base[0]) / 2., // front
                (base[3] + base[2]) / 2., // back
            ];
            if self.tyres_ready {
                for i in 0..4 {
                    towards(&mut self.tyre_heights[i], target[i], 0.5);
                }
            } else {
                self.tyre_heights = target;
                self.tyres_ready = true;
            }
        }
        let [left, right, front, back] = self.tyre_heights;
        self.roll_slope = if self.midair { 0 } else { -atan2s((right - left) / WHEEL_BASE, 1.) };
        if !self.midair {
            self.slope_accel = towards_s16(self.slope_accel, atan2s((back - front) / WHEEL_BASE, 1.), 0.5);
        } else {
            let rise = (self.pos[1] - next_y) / WHEEL_BASE;
            let a = atan2s(rise, 1.);
            let a = if rise >= 0. { a / 4 } else { s16(a as f64 * 10.) };
            self.slope_accel = towards_s16(self.slope_accel, a, 0.5);
        }
    }

    /// One 60 Hz update: input handling then `func_8002D268`.
    pub fn step(&mut self, input: &Input, ground: &dyn Ground) {
        self.bumped = false;
        self.previous_speed = self.speed;
        self.handle_input(input);
        // update_player_drift_duration
        if self.drifting {
            self.drift_duration = (self.drift_duration + 1).min(100);
        } else {
            self.drift_duration = (self.drift_duration - 1).max(0);
        }
        self.mini_turbo_tick();
        if self.drift_outside && self.drift_state_counter >= 100 {
            self.decelerate(4.);
        }
        let sp7c = if self.drift_outside && self.drift_state_counter >= 100 { 2. } else { 0. };
        // func_80037BB4
        let lateral = if self.unk_078 == 0 {
            [0.; 3]
        } else {
            if !self.drift_outside || self.drift_state_counter >= 100 {
                self.rotation = s16((self.rotation + self.unk_078) as f64);
            }
            self.lateral()
        };
        self.gravity_tick();
        self.side_grip_tick();
        // unk_064 (last frame's ground reaction) is flipped while reversing.
        if self.back_up {
            self.ground_force[0] = -self.ground_force[0];
            self.ground_force[2] = -self.ground_force[2];
        }
        let kmh = self.kmh();
        let (mut gx, gy, mut gz);
        if self.midair {
            gx = 0.;
            gy = -self.kart_gravity;
            gz = 0.;
        } else {
            let n = self.normal;
            // Both back tyres on asphalt -> 0.925, otherwise the speed-indexed table (sp104).
            let slope_factor = if !self.tyre_grass[2] && !self.tyre_grass[3] {
                0.925
            } else {
                NON_ASPHALT_GRAVITY[((kmh / 10.) as usize).min(10)]
            };
            gx = -self.ground_force[0] + (-n[0] * self.kart_gravity * slope_factor);
            gy = -n[1] * self.kart_gravity;
            gz = -self.ground_force[2] + (-n[2] * self.kart_gravity * slope_factor);
        }
        // MOVE_BACKWARDS from last frame's displacement; also the brake-hold force (sp178[2]).
        let b = self.rotation + self.unk_0c0;
        let moved_back = (self.old_pos[2] - self.pos[2]) * cos_a(b) + (-(self.old_pos[0] - self.pos[0])) * sin_a(b);
        self.move_backwards = moved_back > 0.1;
        let heading = if self.back_up { self.rotation.wrapping_add(32768) } else { self.rotation };
        let mut sp178 = [0.; 3];
        if self.unk_08c <= 0. && self.braking {
            sp178[2] = moved_back * 4500.;
        }
        let mut sp178 = self.to_world(sp178, heading);
        gx += sp178[0];
        gz += sp178[2];
        // func_80030150 thrust, along the heading (reversed when backing up).
        let thrust = self.thrust();
        let t = self.to_world([0., 0., thrust], heading);
        let friction = 0.12 * FRICTION;
        let mut v = self.velocity;
        let divisor = if (self.steer_position >> 16).abs() >= 40 {
            self.unk_20c * 0.6 + 1. + sp7c
        } else {
            sp7c + 1.
        };
        v[0] += ((t[0] + gx + lateral[0]) - v[0] * friction) / 6000. / divisor;
        v[2] += ((t[2] + gz + lateral[2]) - v[2] * friction) / 6000. / divisor;
        v[1] += ((t[1] + gy + lateral[1]) - v[1] * friction) / 6000. / self.unk_dac;
        let old = self.pos;
        self.old_pos = old;
        let mut next = [old[0] + self.velocity[0], old[1] + self.velocity[1], old[2] + self.velocity[2]];
        // func_8002AAC0
        self.hop_accel = (self.hop_accel - self.hop_jerk).clamp(-9., 9.);
        self.hop_velocity = (self.hop_velocity + self.hop_accel).min(15.);
        if self.hop_velocity <= 0. {
            self.hop_jerk = 0.;
            self.hop_accel = 0.;
            self.hop_velocity = 0.;
        }
        next[1] += self.hop_velocity - 0.02;
        // actor_terrain_collision: surfaceDistance[2] is the signed distance to the floor plane minus the
        // bounding box; the floor only registers within 16 units below the plane.
        let pre = next;
        self.midair = true;
        self.air_frames += 1;
        self.surface_distance = 1000.;
        if let Some(f) = ground.floor(next[0], next[1] + 20., next[2]) {
            let d = f.normal[1] * (next[1] + BOUNDING_BOX - f.height) - BOUNDING_BOX;
            if d > -16. {
                self.surface_distance = d;
                self.normal = f.normal;
                self.grass = f.grass;
            }
        }
        let walls = ground.wall(pre);
        let distance = self.surface_distance;
        if distance <= 0. {
            self.hop = false;
            self.midair = false;
            self.air_frames = 0;
            self.hop_velocity = 0.;
            // func_8003F46C -> func_8003E9EC: push out of the floor, bounce or slide, and the slope reaction.
            let n = self.normal;
            next[0] += n[0] * -distance;
            next[1] += n[1] * -distance * 0.1;
            next[2] += n[2] * -distance;
            reflect(n, distance, &mut v, 1.2, 2.);
            let slope = self.slope_degrees() as f64;
            let roll = (self.roll_slope / DEG) as f64;
            let local = if n[1] <= 0.8357 {
                self.decelerate(4.);
                self.unk_dac = 1.;
                [roll * 120., 0., -slope * 180.]
            } else if self.kmh() > 20. {
                let asphalt = self.tyre_grass.iter().any(|g| !g);
                [roll * if asphalt { 5. } else { 40. }, 0., -slope * 40.]
            } else {
                [0., 0., -slope * 50.]
            };
            sp178 = self.to_world(local, heading);
        }
        // Walls: Z-facing first (surfaceDistance[0], func_8003F734), then X-facing (func_8003FBAC), each followed
        // by func_8002C954's scrub.
        if let Some(w) = &walls {
            let (dx, dz) = (w.pos[0] - pre[0], w.pos[2] - pre[2]);
            next[0] += dx;
            next[2] += dz;
            for x_facing in [false, true] {
                for hit in w.hits.iter().filter(|h| h.x_facing == x_facing) {
                    let n = hit.normal;
                    let depth = (n[0] * dx + n[2] * dz).max(0.);
                    let vertical = n[1].abs() < 0.01;
                    let restitution = if vertical {
                        if self.slope_accel < 0 && self.kmh() < 10. {
                            if x_facing { 1.5 } else { 2.5 }
                        } else {
                            0.5
                        }
                    } else if n[1] <= 0.5 {
                        1.
                    } else {
                        1.2
                    };
                    if !vertical && n[1] <= 0.5 {
                        next[1] += n[1] * depth * 0.1;
                    }
                    reflect(n, -depth.max(1e-9), &mut v, restitution, 0.);
                    if !vertical && !self.midair {
                        v[1] *= -1e-5;
                    }
                    self.drifting = false;
                    self.bumped = true;
                    self.bumps += 1;
                    if self.slope_accel >= 0 && self.kmh() > 5. {
                        self.decelerate(18.);
                    }
                }
            }
        }
        self.sample_tyres(ground, next[1]);
        // func_8002AE38 uses the heading before the speed is updated.
        self.slip_tick(old, next);
        self.speed = (v[0] * v[0] + v[2] * v[2]).sqrt();
        if self.unk_08c <= 0. && self.speed <= 0.08 {
            v[0] = 0.;
            v[2] = 0.;
        } else {
            self.pos[0] = next[0];
            self.pos[2] = next[2];
        }
        self.pos[1] = next[1];
        self.ground_force = sp178;
        self.velocity = v;
        if SPEED_CAP < self.speed {
            let k = SPEED_CAP / self.speed;
            for c in &mut self.velocity {
                *c *= k;
            }
            self.speed = SPEED_CAP;
        }
        if self.back_up && self.speed > 1. {
            let k = 1. / self.speed;
            for c in &mut self.velocity {
                *c *= k;
            }
            self.speed = 1.;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Flat {
        grass: bool,
        wall_x: Option<f64>,
    }
    impl Ground for Flat {
        fn floor(&self, _: f64, _: f64, _: f64) -> Option<Floor> {
            Some(Floor { height: 0., normal: [0., 1., 0.], grass: self.grass })
        }
        fn wall(&self, pos: [f64; 3]) -> Option<Wall> {
            self.wall_x.filter(|x| pos[0] > *x).map(|x| Wall {
                pos: [x, pos[1], pos[2]],
                hits: vec![WallHit { normal: [-1., 0., 0.], x_facing: true }],
            })
        }
    }
    fn road() -> Flat {
        Flat { grass: false, wall_x: None }
    }
    fn go(k: &mut Kart, g: &dyn Ground, n: usize, i: Input) {
        for _ in 0..n {
            k.step(&i, g);
        }
    }
    const GAS: Input = Input { raw_x: 0, raw_y: 80, accel: true, brake: false, hop_down: false, hop_pressed: false };

    #[test]
    fn accelerates_over_about_five_seconds_to_the_decomp_top_speed() {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &g, 60 * 3, GAS);
        let at3 = k.speed;
        go(&mut k, &g, 60 * 6, GAS);
        assert_eq!(k.current_speed, TOP_SPEED);
        // thrust 320^2/25 over 0.12*5800 drag = 5.885 course units per frame.
        assert!((k.speed - 4096. / 696.).abs() < 0.05, "{}", k.speed);
        assert!(at3 > 2.5 && at3 < k.speed - 0.3, "{at3}");
    }
    #[test]
    fn releasing_the_throttle_coasts_down_and_brake_stops_sooner() {
        let g = road();
        let mut a = Kart::new([0.; 3], 0);
        go(&mut a, &g, 600, GAS);
        let mut b = a.clone();
        let coast = Input { accel: false, raw_y: 0, ..GAS };
        go(&mut a, &g, 120, coast);
        let brake = Input { accel: false, brake: true, raw_y: -30, ..GAS };
        go(&mut b, &g, 120, brake);
        assert!(a.speed < 5.0 && a.speed > 0.5, "{}", a.speed);
        assert!(b.speed < a.speed + 0.01, "{} {}", b.speed, a.speed);
    }
    #[test]
    fn grass_costs_most_of_the_speed() {
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &Flat { grass: true, wall_x: None }, 600, GAS);
        assert!(k.speed < 2.6 && k.speed > 1.5, "{}", k.speed);
    }
    #[test]
    fn steering_turns_in_the_stick_direction_and_scrubs_speed() {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &g, 400, GAS);
        let straight = k.speed;
        let r0 = k.rotation;
        go(&mut k, &g, 60, Input { raw_x: 80, ..GAS });
        assert!(k.rotation > r0 + 4000, "{}", k.rotation - r0);
        assert!(k.speed < straight);
        assert!(k.yaw() < 0., "right stick decreases the browser yaw");
        go(&mut k, &g, 120, Input { raw_x: -80, ..GAS });
        assert!(k.rotation < r0 + 4000);
    }
    #[test]
    fn hop_leaves_the_ground_and_lands() {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &g, 400, GAS);
        k.step(&Input { raw_x: 60, hop_pressed: true, hop_down: true, ..GAS }, &g);
        assert!(k.hop && k.midair && k.pos[1] > 0.5);
        assert!(k.drifting);
        let mut peak: f64 = 0.;
        for _ in 0..30 {
            k.step(&Input { raw_x: 60, hop_down: true, ..GAS }, &g);
            peak = peak.max(k.pos[1]);
        }
        assert!(peak > 2. && peak < 8., "{peak}");
        assert!(!k.hop && !k.midair && k.pos[1].abs() < 0.5, "{}", k.pos[1]);
    }
    #[test]
    fn countersteering_a_drift_earns_a_mini_turbo() {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &g, 600, GAS);
        k.step(&Input { raw_x: 70, hop_pressed: true, hop_down: true, ..GAS }, &g);
        go(&mut k, &g, 20, Input { raw_x: 70, hop_down: true, ..GAS });
        assert!(k.drifting);
        let held = Input { hop_down: true, ..GAS };
        for _ in 0..4 {
            go(&mut k, &g, 24, Input { raw_x: -70, ..held });
            go(&mut k, &g, 6, Input { raw_x: 70, ..held });
            if k.drift_state >= 2 {
                break;
            }
        }
        assert!(k.drift_state >= 2, "state {}", k.drift_state);
        go(&mut k, &g, 2, GAS);
        assert!(k.mini_turbo, "releasing the hop button triggers the turbo");
        go(&mut k, &g, 10, GAS);
        assert!(k.unk_0e8 > 100.);
    }
    #[test]
    fn walls_stop_and_scrub_the_kart() {
        let g = Flat { grass: false, wall_x: Some(200.) };
        let mut k = Kart::new([0.; 3], -16384); // facing +x
        go(&mut k, &g, 900, GAS);
        assert!(k.pos[0] <= 200.5);
        assert!(k.current_speed < TOP_SPEED);
    }
    #[test]
    fn reverse_is_slow_and_needs_a_stop() {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        go(&mut k, &g, 120, Input { raw_y: -80, accel: false, brake: true, ..GAS });
        assert!(k.back_up);
        assert!(k.speed > 0.5 && k.speed <= 1.0 + 1e-9, "{}", k.speed);
        assert!(k.pos[2] < -10., "{:?}", k.pos);
    }
    /// A rolling test course shared with `kartPhysics.test.ts`: flat, a 15% climb from z=300 to z=700, a steeper
    /// descent after the crest, a wall at x=-150 (faces +x) and one at z=1500 (faces -z).
    struct Hills;
    impl Hills {
        fn slope(z: f64) -> f64 {
            if z < 300. {
                0.
            } else if z < 700. {
                0.15
            } else {
                -0.3
            }
        }
        fn height(z: f64) -> f64 {
            if z < 300. {
                0.
            } else if z < 700. {
                0.15 * (z - 300.)
            } else {
                60. - 0.3 * (z - 700.)
            }
        }
    }
    impl Ground for Hills {
        fn floor(&self, _: f64, _: f64, z: f64) -> Option<Floor> {
            let g = Self::slope(z);
            let len = (1. + g * g).sqrt();
            Some(Floor { height: Self::height(z), normal: [0., 1. / len, -g / len], grass: false })
        }
        fn wall(&self, pos: [f64; 3]) -> Option<Wall> {
            let mut out = Wall { pos, hits: vec![] };
            if pos[0] < -150. {
                out.pos[0] = -150.;
                out.hits.push(WallHit { normal: [1., 0., 0.], x_facing: true });
            }
            if pos[2] > 1500. {
                out.pos[2] = 1500.;
                out.hits.push(WallHit { normal: [0., 0., -1.], x_facing: false });
            }
            (!out.hits.is_empty()).then_some(out)
        }
    }
    /// The shared trace scenarios; one text line per checkpoint.
    fn trace() -> Vec<String> {
        let g = road();
        let mut k = Kart::new([0.; 3], 0);
        let mut out = vec![];
        for f in 0..600usize {
            let i = Input {
                raw_x: if (200..260).contains(&f) { 80 } else if (300..330).contains(&f) { -60 } else { 0 },
                hop_pressed: f == 420,
                hop_down: (420..470).contains(&f),
                ..GAS
            };
            k.step(&i, &g);
            if f % 100 == 99 {
                out.push(format!("{} {:.4} {:.3} {:.3} {:.3}", f, k.speed, k.pos[0], k.pos[2], k.yaw()));
            }
        }
        // Hills: climb, crest and land, bounce off both walls, reverse, brake on the slope.
        let h = Hills;
        let mut k = Kart::new([0.; 3], 0);
        let (mut bumps, mut air) = (0, 0);
        for f in 0..900usize {
            let i = match f {
                0..=399 => GAS,
                400..=439 => Input { raw_x: 80, ..GAS },
                440..=599 => GAS,
                600..=659 => Input { raw_x: 80, hop_pressed: f == 600, hop_down: true, ..GAS },
                660..=749 => Input { raw_x: 0, raw_y: -80, accel: false, brake: true, ..GAS },
                _ => Input { raw_x: 0, raw_y: 0, accel: false, brake: true, ..GAS },
            };
            k.step(&i, &h);
            bumps += k.bumped as i32;
            air += k.midair as i32;
            if f % 100 == 99 {
                out.push(format!(
                    "H{} {:.4} {:.3} {:.3} {:.3} {:.3} {} {} {}",
                    f, k.speed, k.pos[0], k.pos[1], k.pos[2], k.yaw(), k.slope_accel / DEG, bumps, air
                ));
            }
        }
        out
    }
    #[test]
    fn trace_matches_the_browser_port() {
        // The browser twin asserts the same checkpoints (src/game/experiments/kartPhysics.test.ts).
        let out = trace();
        for line in &out {
            println!("{line}");
        }
        let golden = include_str!("kart_physics_trace.txt");
        assert_eq!(out.join("\n").replace("-0.000", "0.000").trim(), golden.trim());
    }
    #[test]
    fn hills_pitch_the_kart_and_launch_it_off_the_crest() {
        let h = Hills;
        let mut k = Kart::new([0.; 3], 0);
        let (mut max_slope, mut min_slope, mut air, mut high) = (0, 0, 0, 0f64);
        for _ in 0..500 {
            k.step(&GAS, &h);
            max_slope = max_slope.max(k.slope_accel / DEG);
            min_slope = min_slope.min(k.slope_accel / DEG);
            air += k.midair as i32;
            high = high.max(k.pos[1] - Hills::height(k.pos[2]));
        }
        assert!(min_slope <= -4, "climbing reads as negative slopeAccel: {min_slope}");
        assert!(max_slope >= 3, "the descent reads as positive: {max_slope}");
        assert!(air > 3 && high > 1., "crest launches the kart: air {air} high {high}");
        assert!(k.pos[1] < 60. && !k.pos[1].is_nan());
    }
    #[test]
    fn a_stopped_kart_on_a_hill_does_not_run_away() {
        let h = Hills;
        let mut k = Kart::new([0., 0., 500.], 0);
        let brake = Input { raw_x: 0, raw_y: 0, accel: false, brake: true, ..GAS };
        go(&mut k, &h, 300, Input { accel: false, raw_y: 0, ..brake });
        assert!(k.speed < 0.3 && (k.pos[2] - 500.).abs() < 40., "speed {} z {}", k.speed, k.pos[2]);
    }
    #[test]
    fn wall_response_uses_the_decomp_restitution() {
        // Ground impact a wall flat on: 0.5 restitution for a vertical wall, so the kart keeps ~half its normal speed.
        let w = Flat { grass: false, wall_x: Some(200.) };
        let mut k = Kart::new([198., 0., 0.], -16384);
        k.velocity = [5., 0., 0.];
        k.current_speed = TOP_SPEED;
        k.speed = 5.;
        let before = k.velocity[0];
        k.step(&GAS, &w);
        assert!(k.bumped);
        assert!(k.velocity[0] < 0. && k.velocity[0].abs() < before, "bounces back slower: {}", k.velocity[0]);
        assert!(k.current_speed < TOP_SPEED - 10., "func_8002C954 scrub: {}", k.current_speed);
        assert!(!k.drifting);
    }
    #[test]
    fn sliding_along_a_wall_is_not_grabbed() {
        let g = Flat { grass: false, wall_x: Some(200.) };
        let mut k = Kart::new([195., 0., 0.], 0); // facing +z, brushing the x=200 wall
        go(&mut k, &g, 200, GAS);
        assert!(k.pos[2] > 300., "kept driving: {}", k.pos[2]);
    }
    #[test]
    fn brake_hold_stops_the_kart_rolling_back() {
        let h = Hills;
        let mut k = Kart::new([0., 0., 600.], 0);
        // Facing uphill with a bit of speed, then let go and brake: it must not slide back down the 15% hill.
        go(&mut k, &h, 90, GAS);
        go(&mut k, &h, 400, Input { raw_x: 0, raw_y: 0, accel: false, brake: true, ..GAS });
        assert!(k.speed < 0.5, "{}", k.speed);
    }
}
