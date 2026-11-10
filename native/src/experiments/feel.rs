//! Small engine-free game-feel helpers shared by the original modes and the kart views: a trauma-based
//! camera shake, a hit-stop / slow-motion clock for 30 Hz simulations, and a decaying "pop" for HUD
//! counters. Nothing here touches a simulation's rules: a mode only decides *when* its next core tick runs
//! and how the camera and HUD are drawn, so seeded outcomes stay the same.
//!
//! Everything is allocation-free and bounded (mobile friendly).
use super::CamPose;
use bevy::math::DVec3;

/// Trauma-based camera shake: events add trauma (0..1), it decays linearly, and the offset scales with
/// trauma squared so small knocks barely move the picture while big ones kick. The noise is a few
/// incommensurate sines (smooth, no per-frame randomness, the same at any frame rate).
#[derive(Clone, Copy, Debug, Default)]
pub struct Shake {
    trauma: f32,
    t: f32,
}

impl Shake {
    /// Trauma lost per second (a full shake is over in about 0.6 s).
    pub const DECAY: f32 = 1.7;

    pub fn add(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount.max(0.0)).min(1.0);
    }

    /// At least this much trauma (repeated small knocks do not pile up into a big one).
    pub fn at_least(&mut self, amount: f32) {
        self.trauma = self.trauma.max(amount.clamp(0.0, 1.0));
    }

    pub fn trauma(&self) -> f32 {
        self.trauma
    }

    pub fn clear(&mut self) {
        self.trauma = 0.0;
    }

    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.trauma = (self.trauma - Self::DECAY * dt).max(0.0);
        self.t += dt;
        if self.t > 1000.0 {
            self.t -= 1000.0;
        }
    }

    /// Offset in -1..1 on the camera's right / up axes and a roll term, already scaled by trauma squared.
    pub fn offset(&self) -> [f32; 3] {
        let k = self.trauma * self.trauma;
        if k <= 0.0 {
            return [0.0; 3];
        }
        let t = self.t;
        let n = |a: f32, b: f32, p: f32| ((t * a + p).sin() + 0.5 * (t * b + p * 2.3).sin()) / 1.5;
        [k * n(41.0, 67.3, 0.3), k * n(47.9, 73.1, 1.9), k * n(35.3, 58.7, 4.1)]
    }

    /// Moves `pose` by up to `amp` world units at full trauma (eye and look together, so the horizon stays put).
    pub fn apply(&self, pose: &mut CamPose, amp: f64) {
        let o = self.offset();
        if o[0] == 0.0 && o[1] == 0.0 {
            return;
        }
        let fwd = (pose.look - pose.eye).normalize_or_zero();
        let right = fwd.cross(pose.up).normalize_or_zero();
        let up = right.cross(fwd);
        let d: DVec3 = right * (o[0] as f64 * amp) + up * (o[1] as f64 * amp);
        pose.eye += d;
        pose.look += d * 0.6;
    }
}

/// Hit-stop and slow motion for a simulation stepped at a fixed rate. Call [`Pace::step`] once per host
/// step: it says whether the core should tick now. While frozen or between slow-motion ticks,
/// [`Pace::alpha`] maps the host's interpolation factor so the picture keeps moving smoothly.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pace {
    stop: u8,
    slow: u16,
    /// 0 at full speed; 1 on the host step that ticked in slow motion, 2 on the step after it (no tick).
    part: u8,
    held: bool,
}

impl Pace {
    /// Longest freeze a single request can ask for (host steps): keeps a chain of hits from locking the game.
    pub const MAX_STOP: u8 = 6;

    /// Freeze for `frames` host steps (the longer of this and what is already pending).
    pub fn stop(&mut self, frames: u8) {
        self.stop = self.stop.max(frames.min(Self::MAX_STOP));
    }

    /// Half speed for `frames` host steps.
    pub fn slow(&mut self, frames: u16) {
        self.slow = self.slow.max(frames.min(90));
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn frozen(&self) -> bool {
        self.held
    }

    pub fn slowed(&self) -> bool {
        self.slow > 0
    }

    /// 1 on a host step that ticked in slow motion, 2 on the step after it, 0 otherwise (and while frozen).
    pub fn part(&self) -> u8 {
        if self.held { 0 } else { self.part }
    }

    /// True when the core should tick on this host step.
    pub fn step(&mut self) -> bool {
        if self.stop > 0 {
            self.stop -= 1;
            self.held = true;
            return false;
        }
        self.held = false;
        if self.part == 1 {
            // Second half of a slow-motion tick: always finished, so the picture never jumps.
            self.part = 2;
            self.slow = self.slow.saturating_sub(1);
            return false;
        }
        if self.slow > 0 {
            self.slow -= 1;
            self.part = 1;
            return true;
        }
        self.part = 0;
        true
    }

    /// The interpolation factor to draw with, given the host's factor between its last two steps.
    pub fn alpha(&self, host_alpha: f32) -> f32 {
        if self.held {
            1.0
        } else {
            match self.part {
                1 => host_alpha * 0.5,
                2 => 0.5 + host_alpha * 0.5,
                _ => host_alpha,
            }
        }
    }
}

/// A value that jumps to 1 on [`Pop::hit`] and eases back to 0: HUD counters scale by `1 + k * value()`,
/// flashes use it as an alpha.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pop(f32);

impl Pop {
    pub fn hit(&mut self) {
        self.0 = 1.0;
    }
    pub fn update(&mut self, dt: f32, rate: f32) {
        self.0 *= (-dt.clamp(0.0, 0.1) * rate).exp();
        if self.0 < 0.003 {
            self.0 = 0.0;
        }
    }
    pub fn value(&self) -> f32 {
        self.0
    }
    /// Overshooting scale for a counter: 1 at rest, `1 + k` right after a hit.
    pub fn scale(&self, k: f32) -> f32 {
        1.0 + k * self.0
    }
}

/// A lagging "damage" bar: follows `value` up at once, holds while `hold` is true, then drains at `rate` per second.
pub fn drain(ghost: f32, value: f32, hold: bool, dt: f32, rate: f32) -> f32 {
    if ghost <= value {
        value
    } else if hold {
        ghost
    } else {
        (ghost - dt * rate).max(value)
    }
}

/// Ease-out-back from 0 to 1 (overshoots a little): pop-in of cards, banners and pickups.
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    let s = 1.70158;
    1.0 + t * t * ((s + 1.0) * t + s)
}

/// Smoothstep 0..1.
pub fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drained_bar_follows_gains_at_once_and_losses_after_the_hold() {
        assert_eq!(drain(0.4, 0.9, true, 0.1, 0.5), 0.9);
        assert_eq!(drain(0.9, 0.4, true, 0.1, 0.5), 0.9);
        let g = drain(0.9, 0.4, false, 0.1, 0.5);
        assert!((g - 0.85).abs() < 1e-6);
        assert_eq!(drain(0.41, 0.4, false, 0.1, 0.5), 0.4);
    }

    #[test]
    fn shake_decays_to_rest_and_stays_bounded() {
        let mut s = Shake::default();
        assert_eq!(s.offset(), [0.0; 3]);
        s.add(0.7);
        s.add(0.7);
        assert_eq!(s.trauma(), 1.0);
        let mut peak = 0.0f32;
        for _ in 0..120 {
            s.update(1.0 / 60.0);
            let o = s.offset();
            peak = peak.max(o[0].abs()).max(o[1].abs());
            assert!(o.iter().all(|v| v.abs() <= 1.0));
        }
        assert!(peak > 0.2, "a full shake moves the camera");
        assert_eq!(s.trauma(), 0.0);
        assert_eq!(s.offset(), [0.0; 3]);
    }

    #[test]
    fn small_knocks_barely_move_and_do_not_stack_with_at_least() {
        let mut s = Shake::default();
        for _ in 0..10 {
            s.at_least(0.2);
        }
        assert!((s.trauma() - 0.2).abs() < 1e-6);
        s.update(0.01);
        assert!(s.offset().iter().all(|v| v.abs() < 0.05));
    }

    #[test]
    fn shake_moves_the_pose_sideways_not_along_the_view() {
        let mut s = Shake::default();
        s.add(1.0);
        s.update(0.013);
        let mut pose = CamPose { eye: DVec3::new(0.0, 100.0, 500.0), look: DVec3::ZERO, up: DVec3::Y, fov: 45.0, near: 1.0, far: 100.0 };
        let before = pose;
        s.apply(&mut pose, 30.0);
        let d = pose.eye - before.eye;
        assert!(d.length() > 0.0 && d.length() <= 30.0 * 1.5);
        let fwd = (before.look - before.eye).normalize();
        assert!(d.dot(fwd).abs() < 1e-6);
    }

    #[test]
    fn hit_stop_skips_ticks_then_resumes() {
        let mut p = Pace::default();
        assert!(p.step());
        p.stop(3);
        p.stop(2);
        assert_eq!((0..3).filter(|_| p.step()).count(), 0);
        assert!(p.frozen());
        assert_eq!(p.alpha(0.3), 1.0);
        assert!(p.step());
        assert!(!p.frozen());
        assert_eq!(p.alpha(0.3), 0.3);
        p.stop(200);
        assert_eq!((0..20).filter(|_| !p.step()).count(), Pace::MAX_STOP as usize);
    }

    #[test]
    fn slow_motion_ticks_every_other_step_with_a_continuous_alpha() {
        let mut p = Pace::default();
        p.slow(8);
        let mut ticks = 0;
        let mut last = -1.0f32;
        for _ in 0..8 {
            let ticked = p.step();
            ticks += ticked as u32;
            for a in [0.0, 0.5, 0.99] {
                let v = p.alpha(a);
                if ticked && a == 0.0 {
                    last = -1.0;
                }
                assert!(v > last, "alpha runs forward within a core tick");
                assert!((0.0..=1.0).contains(&v));
                last = v;
            }
        }
        assert_eq!(ticks, 4);
        assert!(p.step() && p.step(), "back to full speed");
        assert_eq!(p.alpha(0.25), 0.25);
    }

    #[test]
    fn pop_and_eases() {
        let mut p = Pop::default();
        assert_eq!(p.scale(0.4), 1.0);
        p.hit();
        assert!((p.scale(0.4) - 1.4).abs() < 1e-6);
        for _ in 0..120 {
            p.update(1.0 / 60.0, 9.0);
        }
        assert_eq!(p.value(), 0.0);
        assert_eq!(ease_out_back(0.0), 0.0);
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-6);
        assert!((0.5..1.0).any_overshoot());
        assert_eq!(smooth(0.5), 0.5);
    }

    trait Overshoot {
        fn any_overshoot(self) -> bool;
    }
    impl Overshoot for std::ops::Range<f32> {
        fn any_overshoot(self) -> bool {
            (0..50).any(|i| ease_out_back(self.start + (self.end - self.start) * i as f32 / 50.0) > 1.0)
        }
    }
}
