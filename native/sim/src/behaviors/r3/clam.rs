//! Clam Shell (`macro_clam_shell` -> `bhvClamShell`, Dire, Dire Docks and Jolly Roger Bay): port of
//! `src/game/behaviors/clam.inc.c`.
//!
//! The shell snaps shut on Mario (2 damage, `INTERACT_CLAM_OR_BUBBA`): it opens (tangible at frame 25 of the opening clip,
//! shaking for 10 frames), and once 150 frames have passed with Mario within 500 it closes for 150 frames, going intangible
//! at frame 30 of the closing clip.
//!
//! Left out: the ring of bubbles it releases while closing (`bhvBubbleMaybe`, visual only), the 1.5 vertical scale (the
//! sim scale is uniform), sounds and the mist puff.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::{become_intangible, become_tangible};
use crate::behaviors::g2::obj::*;
use crate::behaviors::r2a::helpers::{hitbox, init_anim_check_frame, obj_check_attacks};
use crate::behaviors::r2a::interact::INTERACT_CLAM_OR_BUBBA;
use crate::mario_runtime::MarioRuntime;
use crate::object::*;

pub const MODEL: &str = "actors/clam_shell/clam_shell_geo.glb";
/// `clam_shell_seg5_anims_05001744`: 0 = open (30 frames), 1 = close (60 frames), both play once.
pub const ANIMS: [AnimSpec; 2] = [spec(0, 0, 0x1E, true), spec(0, 0, 0x3C, true)];
pub const ANIM_NAMES: [&str; 2] = ["clam_shell_seg5_anim_05001654", "clam_shell_seg5_anim_0500172C"];

/// `sClamShellHitbox`.
const HITBOX: ObjectHitbox = hitbox(INTERACT_CLAM_OR_BUBBA, 0.0, 2.0, 99.0, 0.0, 150.0, 80.0, 150.0, 80.0);

pub fn init(o: &mut SimObject) {
    o.visible = true;
    anim::init(o, 0, ANIMS[0]);
}

/// `bhv_clam_loop`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        let o = &mut m.objects[id];
        if first_frame(o) {
            drop_to_floor(o, &world);
        }
        match o.action {
            0 => {
                if init_anim_check_frame(o, &ANIMS, 0, 25) {
                    become_tangible(o);
                    o.set_var("shake", 10.0);
                    o.timer = 0.0;
                } else if o.timer > 150.0 && o.distance_to_mario < 500.0 {
                    o.action = 1;
                } else if o.var("shake") != 0.0 {
                    o.set_var("shake", o.var("shake") - 1.0);
                    // cur_obj_shake_y(3)
                    o.pos[1] += if (o.timer as i64) % 2 == 0 { 3.0 } else { -3.0 };
                }
            }
            _ => {
                if o.timer > 150.0 {
                    o.action = 0;
                } else if init_anim_check_frame(o, &ANIMS, 1, 8) {
                    // The bubble ring is left out.
                } else if anim::check_frame(o, 30) {
                    become_intangible(o);
                }
            }
        }
        let action = o.action;
        obj_check_attacks(o, HITBOX, action, false);
        anim::advance(o);
    });
}
