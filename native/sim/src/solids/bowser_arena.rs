//! The two moving parts of the Bowser arenas: `bhvTiltingBowserLavaPlatform` (Bowser in the Fire
//! Sea) and `bhvFallingBowserPlatform` (Bowser in the Sky), ported from
//! `bowser_falling_platform.inc.c` and the behavior script of the tilting platform.
//! Bowser talks to them through the sim object's vars (`platPitchVel`, `platRollVel`,
//! `platReset`, `platformTag`), which `behaviors::bowser_sm64` writes.
use super::*;
use crate::behaviors::bowser_sm64 as bw;

pub fn make(spec: &SolidSpec) -> Option<Box<dyn Behavior>> {
    Some(match spec.behavior.as_str() {
        "bhvTiltingBowserLavaPlatform" => Box::new(TiltingPlatform),
        "bhvFallingBowserPlatform" => Box::new(FallingPlatform::default()),
        _ => return None,
    })
}

/// `bhvTiltingBowserLavaPlatform`: `SET_INT(oFaceAngleYaw, 0)`, then each frame
/// `cur_obj_rotate_face_angle_using_vel` + `load_object_collision_model`. Bowser's
/// `bowser_tilt_platform` writes `oAngleVelPitch/Roll`; at the end of the tilt he zeroes the
/// angles.
pub struct TiltingPlatform;

impl Behavior for TiltingPlatform {
    fn init(&mut self, o: &mut Obj) { o.face[1] = 0.0; }

    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        if let Some(b) = cx.bowser() {
            o.angle_vel[0] = b.var("platPitchVel");
            o.angle_vel[2] = b.var("platRollVel");
            if b.var("platReset") != 0.0 {
                o.angle_vel[0] = 0.0;
                o.angle_vel[2] = 0.0;
                o.face[0] = 0.0;
                o.face[2] = 0.0;
                return;
            }
        } else {
            o.angle_vel[0] = 0.0;
            o.angle_vel[2] = 0.0;
        }
        o.rotate_face_angle_using_vel();
    }
}

/// `sBowserFallingPlatform`: { posX, posZ, angle } per platform number 1..=10.
pub const FALLING_PLATFORM_DATA: [(f64, f64, f64); 11] = [
    (0.0, 0.0, 0.0),
    (-800.0, -1000.0, -20992.0),
    (-1158.0, 390.0, -18432.0),
    (-1158.0, 390.0, -7680.0),
    (0.0, 1240.0, -6144.0),
    (0.0, 1240.0, 6144.0),
    (1158.0, 390.0, 7680.0),
    (1158.0, 390.0, 18432.0),
    (800.0, -1000.0, 20992.0),
    (800.0, -1000.0, -31744.0),
    (-800.0, -1000.0, 31744.0),
];

/// `BOWSER_BITS_PLAT_ACT_*`.
const PLAT_START: i32 = 0;
const PLAT_CHECK: i32 = 1;
const PLAT_FALL: i32 = 2;

#[derive(Default)]
pub struct FallingPlatform {
    timer: f64,
    pub mist_events: u32,
}

impl Behavior for FallingPlatform {
    /// `bhv_falling_bowser_platform_loop`.
    fn update(&mut self, o: &mut Obj, cx: &Cx) {
        let n = (o.bparam2() as usize).min(10);
        match o.action {
            PLAT_START => {
                if cx.bowser().is_some() {
                    o.action = PLAT_CHECK;
                }
            }
            PLAT_CHECK => {
                if let Some(b) = cx.bowser() {
                    let on_me = b.has_var("platformTag") && b.var("platformTag") == (SOLID_TAG + cx.index as u32) as f64;
                    if on_me && b.action == bw::ACT_BIG_JUMP && (b.var("status") as u32) & bw::STATUS_BIG_JUMP != 0 {
                        o.action = PLAT_FALL;
                    }
                    if b.var("health") == 1.0 && (b.action == bw::ACT_DANCE || b.held_state != crate::object::HELD_FREE) {
                        o.sub_action = 1;
                    }
                }
                if o.sub_action == 0 {
                    self.timer = 0.0;
                } else {
                    // (gDebugInfo[DEBUG_PAGE_EFFECTINFO][6] + 20) * (bhvParam2 - 1) with the debug value 0
                    if 20.0 * (n as f64 - 1.0) < self.timer {
                        o.action = PLAT_FALL;
                    }
                    self.timer += 1.0;
                }
            }
            _ => {
                if o.timer < 22 {
                    o.vel[1] = 8.0;
                    o.gravity = 0.0;
                } else {
                    o.gravity = -4.0;
                }
                if o.timer & 1 == 0 && o.timer < 14 {
                    self.mist_events += 2;
                }
                o.move_using_fvel_and_gravity();
                if o.timer > 300 {
                    o.deleted = true;
                    o.visible = false;
                }
            }
        }
    }
}
