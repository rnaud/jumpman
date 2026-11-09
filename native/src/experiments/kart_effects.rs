//! MK64 kart effects (`apply_triggers` / `apply_effect`): what an item or a collision does to a kart.
//! Engine-free, one `frame` = one 60 Hz kart update, run just before `Kart::step`. Ported from the decomp's
//! `src/effects.c` (`trigger_*`, `apply_*`, `remove_*`, `func_8008C*`), `src/player_controller.c` (`apply_triggers`,
//! `func_8002B5C0` trigger masks, `apply_effect`) and `src/racing/race_logic.c` (`func_802903D8`, kart-vs-kart).
//! The numbers (frame counts, angles, speeds) are the decomp's; `kart_physics.rs` gets small hooks
//! (`boost_power`, `mushroom`, `star`, `lightning`, `thrust_mult`, `yaw_kick`, `launch`).
//!
//! Not ported: the Lakitu effects, boost ramps, terrain tumble triggering, the triple-A boost, battle balloons,
//! the screen-space sprite effects (those are in the views).
use super::kart_physics::{Input, Kart, TOP_SPEED};

// player->effects (include/defines.h)
pub const BRAKING_EFFECT: u32 = 0x1;
pub const MIDAIR_EFFECT: u32 = 0x8;
pub const DRIVING_SPINOUT_EFFECT: u32 = 0x40;
pub const BANANA_SPINOUT_EFFECT: u32 = 0x80;
pub const STAR_EFFECT: u32 = 0x200;
pub const HIT_BY_GREEN_SHELL_EFFECT: u32 = 0x400;
pub const BANANA_NEAR_SPINOUT_EFFECT: u32 = 0x800;
pub const MUSHROOM_EFFECT: u32 = 0x2000;
pub const EARLY_START_SPINOUT_EFFECT: u32 = 0x4000;
pub const ENEMY_BONK_EFFECT: u32 = 0x8000;
pub const LIGHTNING_STRIKE_EFFECT: u32 = 0x20000;
pub const BANANA_SPINOUT_SAVE_EFFECT: u32 = 0x40000;
pub const EXPLOSION_CRASH_EFFECT: u32 = 0x1000000;
pub const HIT_BY_STAR_EFFECT: u32 = 0x2000000;
pub const SQUISH_EFFECT: u32 = 0x4000000;
pub const POST_SQUISH_EFFECT: u32 = 0x8000000;
pub const LIGHTNING_EFFECT: u32 = 0x4000_0000;
pub const BOO_EFFECT: u32 = 0x8000_0000;

// player->triggers
pub const HIT_BANANA_TRIGGER: u32 = 0x1;
pub const HIGH_TUMBLE_TRIGGER: u32 = 0x2;
pub const LOW_TUMBLE_TRIGGER: u32 = 0x4;
pub const THWOMP_SQUISH_TRIGGER: u32 = 0x100;
pub const SHROOM_TRIGGER: u32 = 0x200;
pub const BOO_TRIGGER: u32 = 0x800;
pub const STAR_TRIGGER: u32 = 0x2000;
pub const LIGHTNING_STRIKE_TRIGGER: u32 = 0x4000;
pub const DRAG_ITEM_EFFECT: u32 = 0x40000;
pub const VERTICAL_TUMBLE_TRIGGER: u32 = 0x400000;
pub const HIT_BY_STAR_TRIGGER: u32 = 0x1000000;
pub const START_BOOST_TRIGGER: u32 = 0x2000000;
pub const START_SPINOUT_TRIGGER: u32 = 0x1000_0000;
const RACING_SPINOUT_TRIGGERS: u32 = 0x200081;
const ANY_BOOST_TRIGGERS: u32 = 0x808200;
const STATE_TRANSITION_TRIGGERS: u32 = 0x23800;
const HIT_TRIGGERS: u32 = 0x1404106;

pub const STAR_EFFECT_DURATION: u32 = 10 * 60; // STAR_EFFECT_DURATION 0xA seconds (gCourseTimer is in seconds)
pub const BOO_EFFECT_DURATION: u32 = 7 * 60;
pub const ALPHA_BOO_EFFECT: i32 = 0x60;
const DEG: i32 = 182; // DEGREES(1)
/// `player->boostTimer` set by `trigger_shroom`.
pub const SHROOM_FRAMES: i32 = 0x50;
/// Mario's `D_800E3710` / `D_800E3730` (tumble launch) and `D_800E3790` / `D_800E37B0` (green shell launch).
const TUMBLE_VELOCITY: f64 = 2.2;
const TUMBLE_JERK: f64 = 0.002;
const SHELL_LAUNCH_VELOCITY: f64 = 1.2;
const SHELL_LAUNCH_JERK: f64 = 0.01;
/// `gKartBoundingBoxSizeTable` (Mario).
pub const KART_RADIUS: f64 = 5.5;
/// `sp30` of `func_8008C9EC`: the spin step per frame by stage.
const SPIN_STEP: [i32; 5] = [6 * DEG, 6 * DEG, 12 * DEG, 9 * DEG, 10 * DEG];

fn towards(v: &mut f64, target: f64, rate: f64) {
    *v = (1. - rate) * *v + rate * target;
    if v.abs() < 0.001 {
        *v = 0.;
    }
}
fn s16(v: i64) -> i32 {
    v as i16 as i32
}

/// What the views need to draw a kart under effects.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Kart size multiplier (lightning shrink 0.7).
    pub size: f64,
    /// 0 = fully flat (squish), 1 = normal.
    pub squash: f64,
    /// Boo transparency 0..=255.
    pub alpha: i32,
    /// Tumble pitch in radians (end-over-end, `unk_042`).
    pub tumble: f64,
    /// Star colour phase 0..4 (blue, yellow, red, green) or none; also the lightning flash.
    pub tint: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct Effects {
    pub flags: u32,
    pub triggers: u32,
    pub human: bool,
    /// `unk_0AE`: heading to restore when a spin ends.
    saved_rotation: i32,
    /// `unk_0B2`: spin stage.
    stage: i32,
    /// `D_8018D920`, wraps as u16.
    spin_acc: i32,
    swerve_timer: i32,
    swerve_accel: f64,
    swerve_dir: i32,
    pub boost_timer: i32,
    /// Frames since the star / boo started (the decomp counts seconds of `gCourseTimer`).
    pub star_frames: u32,
    pub boo_frames: u32,
    /// `unk_0B0`: frames of the lightning shrink.
    lightning_frames: i32,
    pub alpha: i32,
    pub size: f64,
    pub bounding_box: f64,
    /// `unk_0A8` / `unk_236` / `unk_0E0` / `unk_042`: tumble counters and the visual pitch.
    tumble_phase: i32,
    tumble_loops: i32,
    tumble_lands: i32,
    pub tumble_angle: i32,
    /// `D_8018D990` and `unk_238`: the squish stage and its timer.
    squish_stage: i32,
    squish_timer: i32,
    flash: i32,
    /// Hit counters for the views / sounds, cleared by `take_started`.
    started: Vec<Started>,
    /// Frames since the effect that ended last (POST_SQUISH grace).
    pub drag: bool,
    /// `EARLY_SPINOUT_LEFT`
    early_left: bool,
}

/// An effect that just began (sound and camera cues).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Started {
    Spinout,
    Banana,
    GreenShell,
    HighTumble,
    VerticalTumble,
    Mushroom,
    Star,
    StarEnd,
    Boo,
    BooEnd,
    Lightning,
    Squish,
    Recovered,
    Bonk,
}

impl Effects {
    pub fn new(human: bool) -> Self {
        Self {
            flags: 0,
            triggers: 0,
            human,
            saved_rotation: 0,
            stage: 0,
            spin_acc: 0,
            swerve_timer: 0,
            swerve_accel: 0.,
            swerve_dir: 1,
            boost_timer: 0,
            star_frames: 0,
            boo_frames: 0,
            lightning_frames: 0,
            alpha: 255,
            size: 1.,
            bounding_box: KART_RADIUS,
            tumble_phase: 0,
            tumble_loops: 0,
            tumble_lands: 0,
            tumble_angle: 0,
            squish_stage: 0,
            squish_timer: 0,
            flash: 0,
            started: vec![],
            drag: false,
            early_left: false,
        }
    }

    pub fn has(&self, f: u32) -> bool {
        self.flags & f == f
    }
    pub fn any(&self, f: u32) -> bool {
        self.flags & f != 0
    }
    pub fn add_trigger(&mut self, t: u32) {
        self.triggers |= t;
    }
    pub fn take_started(&mut self) -> Vec<Started> {
        std::mem::take(&mut self.started)
    }
    /// The kart is under an effect that takes the steering and pedals away (`func_80037CFC`'s guard list).
    pub fn blocks_controls(&self) -> bool {
        self.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT | HIT_BY_GREEN_SHELL_EFFECT | EXPLOSION_CRASH_EFFECT | HIT_BY_STAR_EFFECT | SQUISH_EFFECT | LIGHTNING_STRIKE_EFFECT | EARLY_START_SPINOUT_EFFECT)
    }
    /// Invulnerable to every hit trigger (a star, or a boo which also passes through karts).
    pub fn is_boo(&self) -> bool {
        self.has(BOO_EFFECT)
    }
    pub fn is_star(&self) -> bool {
        self.has(STAR_EFFECT)
    }
    /// `evaluate_collision_between_player_actor`'s per-actor refusals.
    pub fn banana_immune(&self) -> bool {
        self.any(BOO_EFFECT | BANANA_NEAR_SPINOUT_EFFECT | BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) || self.triggers & HIT_BANANA_TRIGGER != 0
    }

    /// `func_8002B5C0`: drop the triggers the current effects refuse.
    fn mask_triggers(&mut self) {
        let f = self.flags;
        let mut t = self.triggers;
        if f & HIT_BY_GREEN_SHELL_EFFECT != 0 {
            t &= !(ANY_BOOST_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS);
        }
        if f & (BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) != 0 {
            t &= !(ANY_BOOST_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS) | 0x20000;
        }
        if f & BANANA_NEAR_SPINOUT_EFFECT != 0 {
            t &= !(ANY_BOOST_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS);
        }
        if f & SQUISH_EFFECT != 0 {
            t &= !(HIT_TRIGGERS | ANY_BOOST_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS) | THWOMP_SQUISH_TRIGGER;
        }
        if f & (EXPLOSION_CRASH_EFFECT | HIT_BY_STAR_EFFECT) != 0 {
            t &= !((HIT_TRIGGERS ^ LIGHTNING_STRIKE_TRIGGER) | ANY_BOOST_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS);
        }
        if f & STAR_EFFECT != 0 {
            t &= !(HIT_TRIGGERS | SHROOM_TRIGGER | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS);
        }
        if f & BOO_EFFECT != 0 {
            t &= !(HIT_TRIGGERS | RACING_SPINOUT_TRIGGERS | STATE_TRANSITION_TRIGGERS);
        }
        self.triggers = t;
    }

    /// `clean_effect`: whatever was running stops before a new effect begins.
    fn clean(&mut self, kart: &mut Kart) {
        if self.has(HIT_BY_GREEN_SHELL_EFFECT) {
            self.end_green_shell(kart);
        }
        if self.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) {
            self.remove_spinout(kart);
        }
        self.flags &= !BANANA_NEAR_SPINOUT_EFFECT;
        if self.has(MUSHROOM_EFFECT) {
            self.remove_mushroom(kart);
        }
        if self.has(EXPLOSION_CRASH_EFFECT) {
            self.flags &= !EXPLOSION_CRASH_EFFECT;
            self.tumble_phase = 0;
            self.tumble_loops = 0;
            self.tumble_angle = 0;
        }
        if self.has(HIT_BY_STAR_EFFECT) {
            self.flags &= !HIT_BY_STAR_EFFECT;
            self.tumble_phase = 0;
            self.tumble_loops = 0;
            self.tumble_angle = 0;
        }
    }

    // ---- spinouts ---------------------------------------------------------------------------------------

    /// `trigger_hit_banana`
    fn trigger_hit_banana(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.triggers &= !HIT_BANANA_TRIGGER;
        self.swerve_timer = 0;
        self.swerve_accel = 3.;
        self.swerve_dir = 1;
        let near = (kart.steer_position >> 16).abs() < 20 && kart.kmh() > 30. && !kart.is_airborne() && self.human;
        // `(type & HUMAN) == 0 && !(LOST_RACE)`: a CPU always spins out.
        if !near {
            self.add_spinout(kart);
        } else {
            self.flags |= BANANA_NEAR_SPINOUT_EFFECT;
        }
        self.started.push(Started::Banana);
    }

    /// `apply_banana_near_spinout_effect`: the swerve before the spin; braking saves you.
    fn near_spinout(&mut self, kart: &mut Kart) {
        self.swerve_timer += 1;
        let t = self.swerve_timer as f64;
        let mut velo = (t * self.swerve_accel - 0.2 * t * t) as i32;
        if self.swerve_timer != 0 && velo < 0 {
            self.swerve_timer = 0;
            self.swerve_dir = -self.swerve_dir;
            self.swerve_accel *= 0.8;
            if kart.braking || self.has(BRAKING_EFFECT) {
                self.flags |= BANANA_SPINOUT_SAVE_EFFECT;
            }
            if self.swerve_accel <= 1. {
                self.flags &= !BANANA_NEAR_SPINOUT_EFFECT;
                if !self.has(BANANA_SPINOUT_SAVE_EFFECT) {
                    self.add_spinout(kart);
                    self.swerve_timer = 0;
                } else {
                    self.flags &= !BANANA_SPINOUT_SAVE_EFFECT;
                    self.started.push(Started::Recovered);
                    self.swerve_timer = 0;
                }
            }
        }
        velo *= self.swerve_dir;
        if (velo <= 0 && self.swerve_dir == 1) || (velo >= 0 && self.swerve_dir == -1) {
            velo = 0;
        }
        kart.yaw_kick += velo * 18;
        if kart.is_airborne() {
            self.add_spinout(kart);
            self.flags &= !BANANA_NEAR_SPINOUT_EFFECT;
        }
    }

    /// `add_spinout_effect`
    pub fn add_spinout(&mut self, kart: &mut Kart) {
        self.clean(kart);
        if self.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) {
            return;
        }
        if kart.slip_raw() / DEG >= 0 {
            self.flags |= DRIVING_SPINOUT_EFFECT;
        } else {
            self.flags |= BANANA_SPINOUT_EFFECT;
        }
        kart.reset_steering();
        self.saved_rotation = kart.rotation;
        self.stage = 2;
        self.spin_acc = 0;
        self.started.push(Started::Spinout);
    }

    /// `remove_spinout_effects`
    fn remove_spinout(&mut self, kart: &mut Kart) {
        self.flags &= !(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT | BANANA_NEAR_SPINOUT_EFFECT);
        kart.rotation = self.saved_rotation;
        kart.reset_steering();
        self.started.push(Started::Recovered);
    }

    /// `func_8008C9EC`: the spin, 360 degrees at 12 per frame then 360 at 6 (90 frames).
    fn spin(&mut self, kart: &mut Kart) {
        if self.human {
            // player_decelerate_alternative(1): the controls are blocked so `Kart::step` already does it.
        } else {
            kart.decelerate_alt(4.);
            kart.decelerate_alt(30.);
        }
        let step = SPIN_STEP[self.stage.clamp(0, 4) as usize];
        if self.has(BANANA_SPINOUT_EFFECT) {
            kart.rotation = s16((kart.rotation - step) as i64);
        } else {
            kart.rotation = s16((kart.rotation + step) as i64);
        }
        self.spin_acc = (self.spin_acc - step) & 0xFFFF;
        let per = 0x10000 / (0x168 / (step / DEG));
        if self.spin_acc / per == 0 {
            self.stage -= 1;
            if self.stage <= 0 {
                self.remove_spinout(kart);
            }
        }
    }

    // ---- tumbles ----------------------------------------------------------------------------------------

    /// `func_8008C528`: hit by a green shell.
    fn trigger_low_tumble(&mut self, kart: &mut Kart) {
        self.clean(kart);
        kart.launch(SHELL_LAUNCH_JERK, SHELL_LAUNCH_VELOCITY);
        self.flags |= HIT_BY_GREEN_SHELL_EFFECT;
        self.tumble_phase = 0;
        self.tumble_loops = 2;
        self.tumble_angle = 0;
        kart.reset_steering();
        self.triggers &= !LOW_TUMBLE_TRIGGER;
        self.started.push(Started::GreenShell);
    }
    /// `func_8008C62C`
    fn low_tumble(&mut self, kart: &mut Kart) {
        kart.decelerate_alt(5.);
        self.tumble_phase += 0xA0;
        self.tumble_angle = s16((self.tumble_angle + 10 * DEG) as i64);
        if self.tumble_phase >= 0x2000 {
            self.tumble_phase = 0;
            self.tumble_loops -= 1;
            if self.tumble_loops == 0 {
                self.end_green_shell(kart);
            }
        }
    }
    /// `func_8008C6D0`
    fn end_green_shell(&mut self, kart: &mut Kart) {
        self.flags &= !HIT_BY_GREEN_SHELL_EFFECT;
        self.tumble_phase = 0;
        self.tumble_loops = 0;
        self.tumble_angle = 0;
        kart.reset_steering();
        kart.current_speed = 0.;
        self.started.push(Started::Recovered);
    }

    /// `trigger_high_tumble`: red shell, blue shell, a kart with a star.
    fn trigger_high_tumble(&mut self, kart: &mut Kart) {
        self.clean(kart);
        kart.launch(TUMBLE_JERK, TUMBLE_VELOCITY);
        self.tumble_phase = 0;
        self.tumble_loops = 4;
        self.tumble_lands = 0;
        self.tumble_angle = 0;
        self.flags |= HIT_BY_STAR_EFFECT;
        self.triggers &= !(HIT_BY_STAR_TRIGGER | HIGH_TUMBLE_TRIGGER);
        self.started.push(Started::HighTumble);
    }
    /// `apply_hit_by_star_effect`: tumbles until it has landed four times or looped four times.
    fn high_tumble(&mut self, kart: &mut Kart) {
        kart.reset_steering();
        self.tumble_angle = s16((self.tumble_angle + 15 * DEG) as i64);
        kart.halve_thrust();
        kart.current_speed = 0.;
        self.flags &= !(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT);
        if !kart.is_airborne() {
            self.tumble_lands += 1;
        }
        if self.tumble_lands == 4 {
            self.end_high_tumble();
        } else {
            self.tumble_phase += 0x90;
            if self.tumble_phase >= 0x2000 {
                self.tumble_phase = 0;
                self.tumble_loops -= 1;
                if self.tumble_loops == 0 {
                    self.end_high_tumble();
                }
            }
        }
    }
    fn end_high_tumble(&mut self) {
        self.flags &= !HIT_BY_STAR_EFFECT;
        self.tumble_phase = 0;
        self.tumble_loops = 0;
        self.tumble_angle = 0;
        self.started.push(Started::Recovered);
    }

    /// `trigger_vertical_tumble`: fake item box, bomb, car.
    fn trigger_vertical_tumble(&mut self, kart: &mut Kart) {
        self.clean(kart);
        kart.launch(TUMBLE_JERK, TUMBLE_VELOCITY);
        self.tumble_phase = 0;
        self.tumble_loops = 4;
        self.tumble_lands = 0;
        self.tumble_angle = 0;
        self.flags |= EXPLOSION_CRASH_EFFECT;
        self.triggers &= !(VERTICAL_TUMBLE_TRIGGER | 0x80000);
        self.started.push(Started::VerticalTumble);
    }
    /// `func_8008E4A4`
    fn vertical_tumble(&mut self, kart: &mut Kart) {
        kart.reset_steering();
        self.tumble_angle = s16((self.tumble_angle + 15 * DEG) as i64);
        kart.halve_thrust();
        kart.cut_thrust();
        kart.current_speed = 0.;
        kart.velocity[0] = 0.;
        kart.velocity[2] = 0.;
        self.flags &= !(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT);
        if !kart.is_airborne() {
            self.tumble_lands += 1;
        }
        if self.tumble_lands == 3 {
            self.end_vertical();
        } else {
            self.tumble_phase += 0x80;
            if self.tumble_phase >= 0x2000 {
                self.tumble_phase = 0;
                self.tumble_loops -= 1;
                if self.tumble_loops == 0 {
                    self.end_vertical();
                }
            }
        }
    }
    fn end_vertical(&mut self) {
        self.flags &= !EXPLOSION_CRASH_EFFECT;
        self.tumble_phase = 0;
        self.tumble_loops = 0;
        self.tumble_angle = 0;
        self.started.push(Started::Recovered);
    }

    // ---- boosts and power-ups ---------------------------------------------------------------------------

    /// `trigger_shroom`
    fn trigger_shroom(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.flags |= MUSHROOM_EFFECT;
        self.triggers &= !SHROOM_TRIGGER;
        self.boost_timer = SHROOM_FRAMES;
        self.started.push(Started::Mushroom);
    }
    /// `apply_mushroom_effect`
    fn mushroom(&mut self, kart: &mut Kart) {
        kart.current_speed = TOP_SPEED;
        if self.boost_timer > 0 {
            self.boost_timer -= 1;
        }
        if self.boost_timer != 0 {
            towards(&mut kart.boost_power, 400., 0.5);
        } else {
            towards(&mut kart.boost_power, 0., 0.1);
        }
        kart.mushroom = true;
        if kart.boost_power <= 1. {
            self.remove_mushroom(kart);
        }
    }
    fn remove_mushroom(&mut self, kart: &mut Kart) {
        self.flags &= !MUSHROOM_EFFECT;
        kart.boost_power = 0.;
        kart.mushroom = false;
    }

    /// `trigger_star`
    fn trigger_star(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.flags |= STAR_EFFECT;
        self.triggers &= !STAR_TRIGGER;
        self.star_frames = 0;
        self.started.push(Started::Star);
    }
    /// `apply_star_effect`
    fn star(&mut self, kart: &mut Kart) {
        self.star_frames += 1;
        kart.star = true;
        if self.star_frames >= STAR_EFFECT_DURATION {
            self.flags &= !STAR_EFFECT;
            kart.star = false;
            self.started.push(Started::StarEnd);
        }
    }

    /// `trigger_boo`
    fn trigger_boo(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.flags |= BOO_EFFECT;
        self.triggers &= !BOO_TRIGGER;
        self.boo_frames = 0;
        self.started.push(Started::Boo);
    }
    /// `apply_boo_effect`: fades to alpha 0x60 for 7 seconds, then back (2 / 4 per frame).
    fn boo(&mut self) {
        self.boo_frames += 1;
        if self.boo_frames < BOO_EFFECT_DURATION {
            self.alpha = (self.alpha - 2).max(ALPHA_BOO_EFFECT);
        } else {
            self.alpha += 4;
            if self.alpha >= 0xF0 {
                self.alpha = 255;
                self.flags &= !BOO_EFFECT;
                self.started.push(Started::BooEnd);
            }
        }
    }

    /// `trigger_lightning_strike`
    fn trigger_lightning(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.triggers &= !LIGHTNING_STRIKE_TRIGGER;
        self.flags |= LIGHTNING_EFFECT | LIGHTNING_STRIKE_EFFECT;
        self.lightning_frames = 0;
        self.size = 1.;
        self.saved_rotation = kart.rotation;
        self.stage = 2;
        self.spin_acc = 0;
        kart.reset_steering();
        kart.halve_thrust();
        self.started.push(Started::Lightning);
    }
    /// `apply_lightning_effect`
    fn lightning(&mut self, kart: &mut Kart) {
        kart.lightning = true;
        if self.has(ENEMY_BONK_EFFECT) && !self.has(SQUISH_EFFECT) {
            // A bump while shrunk squashes the kart (`trigger_squish`).
            self.flags &= !LIGHTNING_STRIKE_EFFECT;
            kart.rotation = self.saved_rotation;
            kart.reset_steering();
            self.remove_lightning(kart);
            self.trigger_squish(kart);
        } else if self.has(LIGHTNING_STRIKE_EFFECT) {
            kart.rotation = s16((kart.rotation - 8 * DEG) as i64);
            self.spin_acc = (self.spin_acc - 8 * DEG) & 0xFFFF;
            if self.spin_acc / (8 * DEG) == 0 {
                self.stage -= 1;
                if self.stage <= 0 {
                    self.flags &= !LIGHTNING_STRIKE_EFFECT;
                    kart.reset_steering();
                    kart.rotation = self.saved_rotation;
                }
            }
            kart.decelerate_alt(1.);
        } else {
            self.lightning_frames += 1;
            kart.thrust_mult = 0.6;
            if self.lightning_frames < 0x1CC {
                towards(&mut self.size, 0.7, 0.1);
                towards(&mut self.bounding_box, KART_RADIUS * 0.9, 0.1);
            } else {
                self.remove_lightning(kart);
                self.started.push(Started::Recovered);
            }
        }
    }
    /// `remove_lightning_effect`
    fn remove_lightning(&mut self, kart: &mut Kart) {
        self.flags &= !(LIGHTNING_EFFECT | LIGHTNING_STRIKE_EFFECT);
        self.flags |= POST_SQUISH_EFFECT;
        self.size = 1.;
        self.bounding_box = KART_RADIUS;
        kart.lightning = false;
        kart.thrust_mult = 1.;
    }

    /// `trigger_squish`
    fn trigger_squish(&mut self, kart: &mut Kart) {
        self.clean(kart);
        if self.has(SQUISH_EFFECT) {
            return;
        }
        self.squish_stage = 0;
        self.squish_timer = 0;
        self.flags &= !POST_SQUISH_EFFECT;
        kart.reset_steering();
        self.flags |= SQUISH_EFFECT;
        self.started.push(Started::Squish);
    }
    /// `apply_hit_effect`: stage 0 (15 frames flat), stage 1 (80 frames, floating up), then back to normal.
    fn squish(&mut self, kart: &mut Kart) {
        kart.reset_steering();
        kart.cut_thrust();
        kart.current_speed = 0.;
        match self.squish_stage {
            0 => {
                self.squish_timer += 2;
                if self.squish_timer >= 0x1E {
                    self.squish_stage = 1;
                    self.squish_timer = 0;
                }
            }
            1 => {
                self.squish_timer += 1;
                if self.squish_timer >= 0x50 {
                    self.squish_stage = 2;
                    self.squish_timer = 0;
                }
            }
            _ => {
                self.flags &= !SQUISH_EFFECT;
                self.flags |= POST_SQUISH_EFFECT;
                self.size = 1.;
                self.bounding_box = KART_RADIUS;
                self.started.push(Started::Recovered);
            }
        }
    }

    /// `func_8002A704`: the start boost is a mushroom boost without `clean_effect`.
    fn trigger_start_boost(&mut self) {
        self.flags |= MUSHROOM_EFFECT;
        self.triggers &= !START_BOOST_TRIGGER;
        self.boost_timer = SHROOM_FRAMES;
        self.started.push(Started::Mushroom);
    }
    /// `func_8008F104`: revving too early wobbles the kart on the line.
    fn trigger_early_spinout(&mut self, kart: &mut Kart) {
        self.clean(kart);
        self.triggers &= !START_SPINOUT_TRIGGER;
        self.saved_rotation = kart.rotation;
        self.stage = 2;
        kart.reset_steering();
        self.flags |= EARLY_START_SPINOUT_EFFECT;
        self.spin_acc = 0x8000;
        self.early_left = false;
        self.started.push(Started::Spinout);
    }
    /// `func_8008F1B8`: swing right 40, left 78, right 78 degrees at 2 per frame (by `unk_0B2`), then settle back
    /// 1 per frame to straight; the speed is a third after.
    fn early_spinout(&mut self, kart: &mut Kart) {
        kart.cut_thrust();
        let two = 2 * DEG;
        if self.stage < 0 {
            let step = if self.early_left { DEG } else { -DEG };
            kart.rotation = s16((kart.rotation + step) as i64);
            self.spin_acc = (self.spin_acc + step) & 0xFFFF;
            if self.spin_acc / DEG == 180 {
                self.flags &= !EARLY_START_SPINOUT_EFFECT;
                kart.current_speed /= 3.;
                self.started.push(Started::Recovered);
            }
        } else if self.stage & 1 != 0 {
            kart.rotation = s16((kart.rotation - two) as i64);
            self.spin_acc = (self.spin_acc - two) & 0xFFFF;
            if self.spin_acc / two < 71 {
                self.stage -= 1;
            }
            self.early_left = true;
        } else {
            kart.rotation = s16((kart.rotation + two) as i64);
            self.spin_acc = (self.spin_acc + two) & 0xFFFF;
            if self.spin_acc / two >= 110 {
                self.stage -= 1;
            }
            self.early_left = false;
        }
    }

    // ---- the frame --------------------------------------------------------------------------------------

    /// `func_8002B830` + `apply_effect` for one 60 Hz frame; call before `Kart::step`. `pad` is zeroed while an
    /// effect has the controls (`func_80037CFC`).
    pub fn frame(&mut self, kart: &mut Kart, pad: &mut Input) {
        self.flags = (self.flags & !MIDAIR_EFFECT) | if kart.is_airborne() { MIDAIR_EFFECT } else { 0 };
        self.flags = (self.flags & !BRAKING_EFFECT) | if kart.braking { BRAKING_EFFECT } else { 0 };
        if self.triggers != 0 {
            self.mask_triggers();
        }
        if self.triggers != 0 {
            let t = self.triggers;
            if t & HIGH_TUMBLE_TRIGGER != 0 {
                self.trigger_high_tumble(kart);
            }
            if t & LOW_TUMBLE_TRIGGER != 0 {
                self.trigger_low_tumble(kart);
            }
            if t & HIT_BANANA_TRIGGER != 0 {
                self.trigger_hit_banana(kart);
            }
            if t & SHROOM_TRIGGER != 0 {
                self.trigger_shroom(kart);
            }
            if t & START_BOOST_TRIGGER != 0 {
                self.trigger_start_boost();
            }
            if t & START_SPINOUT_TRIGGER != 0 {
                self.trigger_early_spinout(kart);
            }
            if t & THWOMP_SQUISH_TRIGGER != 0 {
                self.trigger_squish(kart);
                self.triggers &= !THWOMP_SQUISH_TRIGGER;
            }
            if t & LIGHTNING_STRIKE_TRIGGER != 0 {
                self.trigger_lightning(kart);
            }
            if t & VERTICAL_TUMBLE_TRIGGER != 0 {
                self.trigger_vertical_tumble(kart);
            }
            if t & HIT_BY_STAR_TRIGGER != 0 {
                self.trigger_high_tumble(kart);
            }
            if t & STAR_TRIGGER != 0 {
                self.trigger_star(kart);
            }
            if t & BOO_TRIGGER != 0 {
                self.trigger_boo(kart);
            }
        }
        // apply_effect
        if self.has(BANANA_NEAR_SPINOUT_EFFECT) {
            self.near_spinout(kart);
        }
        if self.has(MUSHROOM_EFFECT) {
            self.mushroom(kart);
        }
        if self.has(SQUISH_EFFECT) {
            self.squish(kart);
        }
        if self.has(LIGHTNING_EFFECT) {
            self.lightning(kart);
        }
        if self.has(STAR_EFFECT) {
            self.star(kart);
        }
        if self.has(BOO_EFFECT) {
            self.boo();
        }
        if self.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) {
            self.spin(kart);
        }
        if self.has(HIT_BY_GREEN_SHELL_EFFECT) {
            self.low_tumble(kart);
        }
        if self.has(EXPLOSION_CRASH_EFFECT) {
            self.vertical_tumble(kart);
        }
        if self.has(HIT_BY_STAR_EFFECT) {
            self.high_tumble(kart);
        }
        if self.has(EARLY_START_SPINOUT_EFFECT) {
            self.early_spinout(kart);
        }
        self.flags &= !ENEMY_BONK_EFFECT;
        self.flash = (self.flash + 5) % 40;
        if self.blocks_controls() {
            *pad = Input::default();
        }
    }

    /// `func_802903D8`'s ENEMY_BONK mark (set on both karts of a plain bump).
    pub fn bonk(&mut self) {
        self.flags |= ENEMY_BONK_EFFECT;
        self.started.push(Started::Bonk);
    }

    /// The tint a kart is drawn with (`change_player_color_effect_rgb`): star colour cycle, lightning flash.
    pub fn look(&self) -> Look {
        let tint = if self.has(LIGHTNING_EFFECT) && self.lightning_frames < 0x78 {
            match self.flash {
                0..=10 => Some([0.5, 0.5, 0.5]),
                11..=20 => Some([0., 0., 0.44]),
                _ => Some([0.56, 0.56, 0.]),
            }
        } else if self.has(STAR_EFFECT) && self.star_frames <= 9 * 60 {
            match self.flash {
                0..=10 => Some([0., 0., 0.44]),
                11..=20 => Some([0.44, 0.44, 0.]),
                21..=30 => Some([0.44, 0., 0.]),
                _ => Some([0., 0.44, 0.]),
            }
        } else {
            None
        };
        let squash = if self.has(SQUISH_EFFECT) {
            match self.squish_stage {
                0 => 1. - 0.75 * ((self.squish_timer + 2) as f64 / 30.).min(1.),
                _ => 0.25,
            }
        } else {
            1.
        };
        Look {
            size: self.size,
            squash,
            alpha: self.alpha,
            tumble: self.tumble_angle as f64 * std::f64::consts::PI / 32768.,
            tint,
        }
    }
}

/// `prevent_item_use` (`src/effects.c`): the effects that refuse an item press. A mushroom also refuses mid-air; a
/// star or boo also refuses while a boo.
pub fn prevents_item(flags: u32, mushroom: bool, star_or_boo: bool) -> bool {
    let block = LIGHTNING_EFFECT | 0x1000_0000 | POST_SQUISH_EFFECT | SQUISH_EFFECT | HIT_BY_STAR_EFFECT | EXPLOSION_CRASH_EFFECT | 0x80_0000 | 0x10_0000 | LIGHTNING_STRIKE_EFFECT | 0x1_0000 | 0x4000 | BANANA_NEAR_SPINOUT_EFFECT | HIT_BY_GREEN_SHELL_EFFECT | STAR_EFFECT | BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT | 0x4;
    if mushroom && flags & MIDAIR_EFFECT != 0 {
        return true;
    }
    let mask = if star_or_boo { block | BOO_EFFECT } else { block };
    flags & mask != 0
}

/// `func_802903D8`: two karts closer than the sum of their radii less 5 bump. Both karts (decomp-space positions,
/// the origin `KART_RADIUS` above the physics origin) are pushed apart, velocity is exchanged scaled by the
/// weights, and a kart with a star sends the other tumbling. Returns whether they touched.
pub fn kart_vs_kart(a: &mut Kart, fa: &mut Effects, wa: f64, b: &mut Kart, fb: &mut Effects, wb: f64) -> bool {
    let (ra, rb) = (fa.bounding_box, fb.bounding_box);
    let reach = ra + rb - 5.;
    let d = [a.pos[0] - b.pos[0], (a.pos[1] - ra) - (b.pos[1] - rb), a.pos[2] - b.pos[2]];
    let rel = [b.velocity[0] - a.velocity[0], b.velocity[1] - a.velocity[1], b.velocity[2] - a.velocity[2]];
    let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if dist < 0.1 {
        return false;
    }
    let overlap = dist - reach;
    if overlap > 0. {
        return false;
    }
    if fa.has(STAR_EFFECT) {
        if !fb.has(STAR_EFFECT) {
            fb.add_trigger(HIT_BY_STAR_TRIGGER);
        }
    } else if fb.has(STAR_EFFECT) {
        fa.add_trigger(HIT_BY_STAR_TRIGGER);
    } else {
        fa.bonk();
        fb.bonk();
    }
    let rel_speed = (rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2]).sqrt();
    let n = [d[0] / dist, d[1] / dist, d[2] / dist];
    if rel_speed < 0.2 {
        let r = (ra + rb) * 0.55;
        a.pos = [b.pos[0] + n[0] * r, b.pos[1] + n[1] * r, b.pos[2] + n[2] * r];
        b.pos = [b.pos[0] - r * n[0], b.pos[1] - r * n[1], b.pos[2] - r * n[2]];
        return true;
    }
    let along = (n[0] * rel[0] + n[1] * rel[1] + n[2] * rel[2]) / rel_speed;
    let impulse = rel_speed * along * 0.85;
    if !fa.has(STAR_EFFECT) {
        let k = impulse * wb / wa;
        for i in 0..3 {
            a.velocity[i] += n[i] * k;
            a.pos[i] -= n[i] * overlap * 0.5;
        }
    }
    if !fb.has(STAR_EFFECT) {
        let k = impulse * wa / wb;
        for i in 0..3 {
            b.velocity[i] -= n[i] * k;
            b.pos[i] += n[i] * overlap * 0.5;
        }
    }
    true
}

/// `gFloatArray802B8790`: kart weight by character id (Mario, Luigi, Yoshi, Toad, DK, Wario, Peach, Bowser).
pub const KART_WEIGHT: [f64; 8] = [1.2, 1.0, 0.9, 0.7, 2.0, 1.8, 0.9, 2.3];

#[cfg(test)]
mod tests {
    use super::super::kart_physics::{Floor, Ground, Wall};
    use super::*;

    struct Flat;
    impl Ground for Flat {
        fn floor(&self, _: f64, _: f64, _: f64) -> Option<Floor> {
            Some(Floor::flat(0., false))
        }
        fn wall(&self, _: [f64; 3]) -> Option<Wall> {
            None
        }
    }
    fn kart() -> Kart {
        Kart::new([0., 0., 0.], 0)
    }
    fn drive(k: &mut Kart, n: usize) {
        let mut fx = Effects::new(true);
        for _ in 0..n {
            let mut pad = Input { accel: true, raw_y: 80, ..Input::default() };
            fx.frame(k, &mut pad);
            k.step(&pad, &Flat);
        }
    }
    fn run(k: &mut Kart, fx: &mut Effects, n: usize, accel: bool) {
        for _ in 0..n {
            let mut pad = Input { accel, raw_y: if accel { 80 } else { 0 }, ..Input::default() };
            fx.frame(k, &mut pad);
            k.step(&pad, &Flat);
        }
    }
    fn frames_until(k: &mut Kart, fx: &mut Effects, flag: u32, limit: usize) -> usize {
        for i in 0..limit {
            if !fx.has(flag) {
                return i;
            }
            run(k, fx, 1, true);
        }
        limit
    }

    #[test]
    fn decomp_constants() {
        // include/defines.h
        assert_eq!(STAR_EFFECT_DURATION, 600);
        assert_eq!(BOO_EFFECT_DURATION, 420);
        assert_eq!(ALPHA_BOO_EFFECT, 0x60);
        assert_eq!(SPIN_STEP[2], 12 * 182);
        assert_eq!(HIT_TRIGGERS, 0x01404106);
        assert_eq!(ANY_BOOST_TRIGGERS, 0x00808200);
        assert_eq!(RACING_SPINOUT_TRIGGERS, 0x200081);
        assert_eq!(STATE_TRANSITION_TRIGGERS, 0x00023800);
        // gFloatArray802B8790
        assert_eq!(KART_WEIGHT[0], 1.2);
        assert_eq!(KART_WEIGHT[7], 2.3);
    }

    #[test]
    fn a_banana_spinout_turns_two_full_circles_in_90_frames_and_restores_the_heading() {
        let mut k = kart();
        drive(&mut k, 200);
        let heading = k.rotation;
        let mut fx = Effects::new(false); // a CPU always spins out
        fx.add_trigger(HIT_BANANA_TRIGGER);
        let mut total = 0;
        let mut last = k.rotation;
        let mut frames = 0;
        loop {
            run(&mut k, &mut fx, 1, true);
            frames += 1;
            if !fx.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT) {
                break;
            }
            let d = s16((k.rotation - last) as i64);
            total += d.abs();
            last = k.rotation;
            assert!(frames < 200);
        }
        assert_eq!(frames, 90, "12 deg x 30 frames then 6 deg x 60");
        assert!((total - 720 * 182).abs() < 8 * 182, "two circles: {}", total / 182);
        assert_eq!(k.rotation, heading, "unk_0AE is restored");
    }

    #[test]
    fn a_human_hitting_a_banana_while_steering_straight_swerves_first() {
        let mut k = kart();
        drive(&mut k, 300);
        assert!(k.kmh() > 30.);
        let mut fx = Effects::new(true);
        fx.add_trigger(HIT_BANANA_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(BANANA_NEAR_SPINOUT_EFFECT), "trigger_hit_banana: no spin yet");
        let mut saw_spin = false;
        for _ in 0..200 {
            run(&mut k, &mut fx, 1, true);
            saw_spin |= fx.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT);
        }
        assert!(saw_spin, "without braking the swerve ends in the spin");
    }

    #[test]
    fn braking_during_the_swerve_saves_the_kart() {
        let mut k = kart();
        drive(&mut k, 300);
        let mut fx = Effects::new(true);
        fx.add_trigger(HIT_BANANA_TRIGGER);
        let mut spun = false;
        for _ in 0..400 {
            let mut pad = Input { accel: true, brake: true, raw_y: 80, ..Input::default() };
            fx.frame(&mut k, &mut pad);
            k.step(&pad, &Flat);
            spun |= fx.any(BANANA_SPINOUT_EFFECT | DRIVING_SPINOUT_EFFECT);
        }
        assert!(!spun, "BANANA_SPINOUT_SAVE_EFFECT");
        assert!(!fx.has(BANANA_NEAR_SPINOUT_EFFECT));
    }

    #[test]
    fn a_green_shell_launches_and_tumbles_for_about_104_frames() {
        let mut k = kart();
        drive(&mut k, 300);
        let mut fx = Effects::new(true);
        fx.add_trigger(LOW_TUMBLE_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(HIT_BY_GREEN_SHELL_EFFECT));
        assert!(k.is_airborne(), "kartHopVelocity launches it");
        let frames = 1 + frames_until(&mut k, &mut fx, HIT_BY_GREEN_SHELL_EFFECT, 400);
        // 2 loops of 0x2000 / 0xA0 (51.2 -> 52) frames
        assert!((100..=108).contains(&frames), "frames {frames}");
        assert!(k.current_speed < 5., "func_8008C6D0 zeroes currentSpeed: {}", k.current_speed);
    }

    #[test]
    fn a_red_shell_tumbles_through_four_loops_and_a_fake_box_through_three_landings() {
        let mut k = kart();
        drive(&mut k, 300);
        let mut fx = Effects::new(true);
        fx.add_trigger(HIGH_TUMBLE_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(HIT_BY_STAR_EFFECT));
        let h = frames_until(&mut k, &mut fx, HIT_BY_STAR_EFFECT, 1000);
        assert!(h > 10 && h < 400, "high tumble ends after four grounded frames or four loops: {h}");
        let mut fx = Effects::new(true);
        let mut k2 = kart();
        drive(&mut k2, 300);
        fx.add_trigger(VERTICAL_TUMBLE_TRIGGER);
        run(&mut k2, &mut fx, 1, true);
        assert!(fx.has(EXPLOSION_CRASH_EFFECT));
        let v = frames_until(&mut k2, &mut fx, EXPLOSION_CRASH_EFFECT, 1000);
        assert!(v > 8 && v < 400, "vertical tumble ends: {v}");
    }

    #[test]
    fn a_mushroom_boosts_for_80_frames_and_beats_the_unboosted_speed() {
        let mut plain = kart();
        drive(&mut plain, 400);
        let base = plain.speed;
        let mut k = kart();
        drive(&mut k, 400);
        let mut fx = Effects::new(true);
        fx.add_trigger(SHROOM_TRIGGER);
        run(&mut k, &mut fx, 30, true);
        assert!(fx.has(MUSHROOM_EFFECT));
        assert!(k.speed >= base - 1e-6, "boosted {} vs {}", k.speed, base);
        assert!(k.boost_power > 300., "boostPower approaches 400: {}", k.boost_power);
        let n = frames_until(&mut k, &mut fx, MUSHROOM_EFFECT, 400);
        assert!(n >= 40 && n < 140, "ends after boostTimer 0x50 plus the fade: {n}");
        assert_eq!(k.boost_power, 0.);
    }

    #[test]
    fn a_star_lasts_ten_seconds_blocks_hits_and_a_boo_seven() {
        let mut k = kart();
        let mut fx = Effects::new(true);
        fx.add_trigger(STAR_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(STAR_EFFECT));
        fx.add_trigger(HIGH_TUMBLE_TRIGGER | HIT_BANANA_TRIGGER | LOW_TUMBLE_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(!fx.any(HIT_BY_STAR_EFFECT | HIT_BY_GREEN_SHELL_EFFECT | BANANA_SPINOUT_EFFECT | BANANA_NEAR_SPINOUT_EFFECT), "func_8002B5C0 masks the hit triggers during a star");
        let n = frames_until(&mut k, &mut fx, STAR_EFFECT, 2000);
        assert!((597..=601).contains(&(n + 2)), "{n}");
        let mut k = kart();
        let mut fx = Effects::new(true);
        fx.add_trigger(BOO_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(BOO_EFFECT));
        run(&mut k, &mut fx, 120, true);
        assert_eq!(fx.alpha, ALPHA_BOO_EFFECT.max(255 - 2 * 121), "alpha falls 2 per frame to 0x60");
        run(&mut k, &mut fx, 120, true);
        assert_eq!(fx.alpha, ALPHA_BOO_EFFECT);
        fx.add_trigger(HIGH_TUMBLE_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(!fx.has(HIT_BY_STAR_EFFECT), "a boo cannot be hit");
        let n = frames_until(&mut k, &mut fx, BOO_EFFECT, 2000);
        assert!((420 - 242 + 25..420 - 242 + 90).contains(&n), "7 s, then the fade-in: {n}");
        assert_eq!(fx.alpha, 255);
    }

    #[test]
    fn lightning_spins_two_circles_then_shrinks_to_0_7_for_460_frames() {
        let mut k = kart();
        drive(&mut k, 300);
        let start = k.rotation;
        let mut fx = Effects::new(true);
        fx.add_trigger(LIGHTNING_STRIKE_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(LIGHTNING_EFFECT | LIGHTNING_STRIKE_EFFECT));
        let spin = 1 + frames_until(&mut k, &mut fx, LIGHTNING_STRIKE_EFFECT, 200);
        assert!((88..=92).contains(&spin), "two circles of 8 deg: {spin}");
        assert_eq!(k.rotation, start);
        run(&mut k, &mut fx, 100, true);
        assert!((fx.size - 0.7).abs() < 0.02, "size {}", fx.size);
        assert!(k.lightning && k.thrust_mult == 0.6);
        let n = frames_until(&mut k, &mut fx, LIGHTNING_EFFECT, 1000);
        assert!((350..=370).contains(&n), "460 frames total: {n}");
        assert_eq!(fx.size, 1.);
        assert!(!k.lightning);
        assert!(fx.has(POST_SQUISH_EFFECT));
    }

    #[test]
    fn lightning_slows_the_kart_down() {
        let mut plain = kart();
        drive(&mut plain, 700);
        let mut k = kart();
        let mut fx = Effects::new(true);
        run(&mut k, &mut fx, 300, true);
        fx.add_trigger(LIGHTNING_STRIKE_TRIGGER);
        run(&mut k, &mut fx, 400, true);
        assert!(k.speed < plain.speed * 0.95, "{} vs {}", k.speed, plain.speed);
    }

    #[test]
    fn a_bump_while_shrunk_squishes_the_kart_flat_for_about_95_frames() {
        let mut k = kart();
        let mut fx = Effects::new(true);
        fx.add_trigger(LIGHTNING_STRIKE_TRIGGER);
        run(&mut k, &mut fx, 120, true);
        fx.bonk();
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(SQUISH_EFFECT));
        assert!(fx.look().squash < 1.);
        let n = frames_until(&mut k, &mut fx, SQUISH_EFFECT, 300);
        assert!((90..=100).contains(&n), "15 + 80 frames: {n}");
        assert_eq!(fx.look().squash, 1.);
    }

    #[test]
    fn the_start_boost_is_a_mushroom_and_the_early_rev_wobbles_for_about_138_frames() {
        let mut k = kart();
        let mut fx = Effects::new(true);
        fx.add_trigger(START_BOOST_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(MUSHROOM_EFFECT) && fx.boost_timer == SHROOM_FRAMES - 1);
        let mut k = kart();
        let start = k.rotation;
        let mut fx = Effects::new(true);
        fx.add_trigger(START_SPINOUT_TRIGGER);
        run(&mut k, &mut fx, 1, true);
        assert!(fx.has(EARLY_START_SPINOUT_EFFECT) && fx.blocks_controls());
        let n = 1 + frames_until(&mut k, &mut fx, EARLY_START_SPINOUT_EFFECT, 400);
        // 20 + 39 + 39 frames of swinging at 2 degrees, then 40 back at 1
        assert!((130..=145).contains(&n), "{n}");
        assert!(s16((k.rotation - start) as i64).abs() < 3 * DEG, "back to straight");
    }

    #[test]
    fn kart_vs_kart_pushes_both_apart_and_a_star_wins() {
        let mut a = Kart::new([0., 5.5, 0.], 0);
        let mut b = Kart::new([5., 5.5, 0.], 0);
        a.velocity = [2., 0., 0.];
        b.velocity = [-2., 0., 0.];
        let (mut fa, mut fb) = (Effects::new(true), Effects::new(false));
        assert!(kart_vs_kart(&mut a, &mut fa, 1.2, &mut b, &mut fb, 0.7));
        assert!(fa.has(ENEMY_BONK_EFFECT) && fb.has(ENEMY_BONK_EFFECT) || fa.started.contains(&Started::Bonk));
        assert!(a.velocity[0] < 2. && b.velocity[0] > -2.);
        assert!(b.pos[0] - a.pos[0] > 5.);
        // the heavier kart is pushed less: Mario 1.2 vs Toad 0.7
        assert!((2. - a.velocity[0]).abs() < (-2. - b.velocity[0]).abs());
        let mut a = Kart::new([0., 5.5, 0.], 0);
        let mut b = Kart::new([5., 5.5, 0.], 0);
        let (mut fa, mut fb) = (Effects::new(true), Effects::new(false));
        fa.flags |= STAR_EFFECT;
        a.velocity = [2., 0., 0.];
        b.velocity = [-2., 0., 0.];
        kart_vs_kart(&mut a, &mut fa, 1.2, &mut b, &mut fb, 1.2);
        assert_eq!(fb.triggers & HIT_BY_STAR_TRIGGER, HIT_BY_STAR_TRIGGER, "playerTwo->triggers |= HIT_BY_STAR_TRIGGER");
        assert_eq!(a.velocity[0], 2., "the star kart keeps going");
        // too far apart: nothing
        let mut c = Kart::new([30., 5.5, 0.], 0);
        assert!(!kart_vs_kart(&mut a, &mut fa, 1.2, &mut c, &mut Effects::new(false), 1.0));
    }
}
