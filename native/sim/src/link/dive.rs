//! Link's dive (`Player_Action_8084DC48`, z_player.c): A at the surface takes him under; holding A keeps him sinking at 2 units a
//! frame while the water is shallower than the Zora Scale allows (120 / 240 / 360 units for no scale / silver / gold), letting go
//! turns him up, and he rises at `depth * 0.018 + 4` (at most 8) units a frame until he breaks the surface (68 units of depth for
//! the adult, 48 for the child), where the surface swim of the SM64 water actions takes over.
//!
//! The decomp runs at 20 Hz: [`LinkExt::run_dive`] executes two actor frames per three ticks. All distances are OoT units
//! (`x4.4` in world units). The strokes are the animation's: speed only builds while the swim animation (29 frames) is between its
//! 10th and 20th frame (`func_8084AEEC`), which gives the swimming its pulse.
use super::items::*;
use super::moves::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::state::*;

const U: f64 = 4.4;
const FRAME_PER_TICK: f64 = 20.0 / 30.0;
/// `D_80854784`: the depth a dive may reach by Zora Scale level.
pub const DEPTH_LIMIT: [f64; 3] = [120.0, 240.0, 360.0];
/// `ageProperties->unk_30`: the depth at which a rising dive has surfaced.
pub const SURFACED: [f64; 2] = [68.0, 48.0];
const SWIM_FRAMES: f64 = 29.0;
const DEEP_START_FRAMES: f64 = 50.0;
/// `R_RUN_SPEED_LIMIT / 100 * 0.8`.
const SPEED_CAP: f64 = 4.8;

#[derive(Clone, Debug, Default)]
pub struct Dive {
    acc: f64,
    /// `actionVar1`: 0 sinking (after the start), 1 settling (the head comes up), 2 rising.
    phase: u8,
    /// `actionVar2`: 0 while the start animation plays, 1 after, then the depth rising begins at.
    started: bool,
    depth_at_rise: f64,
    /// `unk_6C2`: the pitch of his body, s16 units.
    pitch: f64,
    /// Frames of the current animation.
    frame: f64,
    pub vy: f64,
    speed: f64,
    yaw: f64,
}

/// `Math_AsymStepToF`.
fn asym_step(value: &mut f64, target: f64, up: f64, down: f64) {
    let step = if target >= *value { up } else { down };
    if step == 0.0 {
        return;
    }
    let step = if target < *value { -step } else { step };
    *value += step;
    if (*value - target) * step >= 0.0 {
        *value = target;
    }
}

/// `Math_StepToF`.
fn step_to(value: &mut f64, target: f64, step: f64) -> bool {
    if *value == target {
        return true;
    }
    if *value < target {
        *value = (*value + step).min(target);
    } else {
        *value = (*value - step).max(target);
    }
    *value == target
}

/// `Math_ScaledStepToS` on a plain number.
fn scaled_step(value: &mut f64, target: f64, step: f64) {
    step_to(value, target, step);
}

impl Dive {
    pub fn depth_limit(scale: u8) -> f64 {
        DEPTH_LIMIT[(scale as usize).min(2)]
    }

    /// `func_8084AEEC`: speed builds only during the stroke, 10 to 20 frames into the 29 of the swim animation.
    fn stroke(&mut self, speed: &mut f64, target: f64, yaw_target: f64) {
        let mut temp = self.frame - 10.0;
        let mut target = target;
        if *speed > SPEED_CAP {
            *speed = SPEED_CAP;
        }
        if temp > 0.0 && temp < 10.0 {
            temp *= 6.0;
        } else {
            temp = 0.0;
            target = 0.0;
        }
        asym_step(speed, target * 0.8, temp, speed.abs() * 0.02 + 0.05);
        // `Math_ScaledStepToS(&this->yaw, yaw_target, 1600)`
        let d = to_s16(yaw_target - self.yaw) as f64;
        self.yaw = to_s16(self.yaw + d.clamp(-1600.0, 1600.0)) as f64;
    }
}

impl LinkExt {
    /// The conditions of `func_8083D12C` for starting a dive from the surface swim.
    pub fn can_dive(&self, m: &MarioRuntime) -> bool {
        self.enabled && self.mv == Mv::Free && m.action & ACT_FLAG_SWIMMING != 0 && self.inv.boots != Boots::Iron && self.epona.rider == super::epona::Rider::None
    }

    pub fn begin_dive(&mut self, m: &mut MarioRuntime) {
        self.mv = Mv::Dive;
        self.timer = 0.0;
        self.dive = Dive { yaw: m.face_angle[1], speed: m.forward_vel.min(SPEED_CAP), ..Default::default() };
        m.vel = [0.0; 3];
    }

    fn depth(&self, m: &MarioRuntime) -> f64 {
        (m.world.water_level(m.pos[0], m.pos[2]) - m.pos[1]) / U
    }

    /// One tick of the dive.
    pub(super) fn run_dive(&mut self, m: &mut MarioRuntime, down: u32) -> bool {
        self.timer += 1.0;
        let age = if self.age == Age::Adult { 0 } else { 1 };
        let mut d = std::mem::take(&mut self.dive);
        d.acc += FRAME_PER_TICK;
        let (mag, yaw_in) = (m.controller.stick_mag, m.intended_yaw);
        let a_held = down & A_BUTTON != 0;
        let press = m.controller.button_pressed & (A_BUTTON | B_BUTTON) != 0;
        let limit = Dive::depth_limit(self.inv.scale);
        let mut surfaced = false;
        let mut frames = 0;
        while d.acc >= 1.0 {
            d.acc -= 1.0;
            frames += 1;
            let depth = self.depth(m);
            // the stick: `speedTarget = magnitude * 0.8 * 0.14`, clamped to the speed cap
            let target = (mag * 0.8 * 0.14).clamp(0.0, SPEED_CAP.max(18.0));
            if self.inv.boots == Boots::Iron {
                surfaced = true;
                break;
            }
            if !d.started {
                // the start animation: at frame 20 he kicks down, and it ends on its last frame or at frame 22 with A let go
                d.frame += 1.0;
                if d.frame >= DEEP_START_FRAMES || (d.frame >= 22.0 && !a_held) {
                    d.started = true;
                    d.pitch = 16000.0;
                    d.frame = 0.0;
                } else if d.frame == 20.0 {
                    d.vy = -2.0;
                }
                step_to(&mut d.speed, 0.0, 8.0);
            } else if d.phase == 0 {
                d.pitch = 16000.0;
                // `func_8084B158`: the stroke animation runs at least at normal speed
                d.frame = (d.frame + if press { 1.0 } else { 1.0 }) % SWIM_FRAMES;
                let on_ground = m.floor_height >= m.pos[1] - 1.0 && m.pos[1] - m.floor_height < 1.0;
                if a_held && !on_ground && depth < limit {
                    // `func_8084DBC4(-2.0)`
                    let mut s = d.speed;
                    d.stroke(&mut s, target * 0.5, yaw_in);
                    d.speed = s;
                    let mut vy = d.vy;
                    let mut dd = Dive { frame: d.frame, yaw: d.yaw, ..Default::default() };
                    dd.stroke(&mut vy, -2.0, d.yaw);
                    d.vy = vy;
                } else {
                    d.phase = 1;
                }
            } else if d.phase == 1 {
                // the pitch comes back by 400 a frame (`Math_ScaledStepToS(&unk_6C2, 0, 400)` in the main update) until it is under 10000
                d.frame = (d.frame + 1.0) % SWIM_FRAMES;
                scaled_step(&mut d.pitch, 0.0, 400.0);
                buoyancy(&mut d.vy, depth, age);
                if d.pitch < 10000.0 {
                    d.phase = 2;
                    d.depth_at_rise = depth;
                    d.frame = 0.0;
                }
            } else {
                // rising: `depth * 0.018 + 4`, at most 8
                d.frame = (d.frame + 1.0) % SWIM_FRAMES;
                let up = (d.depth_at_rise * 0.018 + 4.0).min(8.0);
                scaled_step(&mut d.pitch, -10000.0, 800.0);
                let mut s = d.speed;
                d.stroke(&mut s, target * 0.5, yaw_in);
                d.speed = s;
                let mut vy = d.vy;
                let mut dd = Dive { frame: d.frame, yaw: d.yaw, ..Default::default() };
                dd.stroke(&mut vy, up, d.yaw);
                d.vy = vy;
                // `func_8083D12C`: moving up and shallower than `unk_30`: he has surfaced
                if d.vy > 0.0 && depth < SURFACED[age] {
                    surfaced = true;
                    break;
                }
            }
            // move: speed along his heading, vy up, in OoT units a frame
            let (sx, sz) = (sins(d.yaw) * d.speed * U, coss(d.yaw) * d.speed * U);
            let mut next = [m.pos[0] + sx, m.pos[1] + d.vy * U, m.pos[2] + sz];
            m.world.resolve_and_return_wall_collisions(&mut next, 30.0, 40.0);
            let floor = m.world.find_floor(next[0], next[1] + 80.0, next[2]);
            if floor.surface.is_some() && next[1] < floor.height {
                next[1] = floor.height;
                if d.vy < 0.0 {
                    d.vy = 0.0;
                }
            }
            m.pos = next;
        }
        let _ = frames;
        m.face_angle[1] = d.yaw;
        m.forward_vel = d.speed * U;
        self.pose = Some(PoseHint { clip: if !d.started { "diveStart" } else if d.phase == 1 { "swimWait" } else { "swim" }, frame: d.frame * 1.5, looping: d.started });
        self.dive = d;
        if surfaced {
            // back to the surface swim
            self.mv = Mv::Free;
            let level = m.world.water_level(m.pos[0], m.pos[2]);
            m.pos[1] = m.pos[1].min(level);
            m.vel = [0.0; 3];
            m.set_action(ACT_WATER_IDLE, 0.0);
        } else {
            m.vel = [0.0; 3];
        }
        true
    }
}

/// `func_8084B000`: the water's lift and drag on a diver that is no longer swimming (gravity is off).
fn buoyancy(vy: &mut f64, depth: f64, age: usize) {
    let float_depth = [44.8, 29.6][age];
    let mut limit = -5.0;
    let accel;
    if depth < float_depth + if *vy < 0.0 { 1.0 } else { 0.0 } {
        let k = if *vy <= 0.0 { 0.0 } else { *vy * 0.5 };
        accel = -0.1 - k;
    } else {
        limit = 2.0;
        let k = if *vy >= 0.0 { 0.0 } else { *vy * -0.3 };
        accel = k + 0.1;
    }
    *vy += accel;
    if (*vy - limit) * accel > 0.0 {
        *vy = limit;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zora_scale_sets_the_depth_limit() {
        assert_eq!([0u8, 1, 2].map(Dive::depth_limit), [120.0, 240.0, 360.0]);
        assert_eq!(Dive::depth_limit(9), 360.0);
    }

    #[test]
    fn strokes_only_push_during_frames_10_to_20_of_the_swim_animation() {
        let mut d = Dive { frame: 5.0, ..Default::default() };
        let mut speed = 0.0;
        d.stroke(&mut speed, 4.0, 0.0);
        assert_eq!(speed, 0.0, "the glide: no push before frame 10");
        d.frame = 15.0;
        d.stroke(&mut speed, 4.0, 0.0);
        assert!((speed - 3.2).abs() < 1e-9, "frame 15: step 30 toward 4 * 0.8: {speed}");
        d.frame = 25.0;
        let before = speed;
        d.stroke(&mut speed, 4.0, 0.0);
        assert!(speed < before && speed > before - 0.2, "after the stroke only drag slows it a little");
    }

    #[test]
    fn buoyancy_lifts_a_deep_diver_gently_and_pushes_a_shallow_one_down() {
        let mut vy = -2.0;
        buoyancy(&mut vy, 200.0, 0);
        assert!((vy - (-2.0 + 2.0 * 0.3 + 0.1)).abs() < 1e-9, "{vy}");
        let mut up = 5.0;
        buoyancy(&mut up, 200.0, 0);
        assert_eq!(up, 2.0, "capped at 2 a frame");
        let mut shallow = 1.0;
        buoyancy(&mut shallow, 10.0, 0);
        assert!(shallow < 1.0);
    }
}
