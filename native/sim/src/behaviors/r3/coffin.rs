//! Big Boo's Haunt coffins (`bhvCoffinSpawner`, `bhvCoffin`): port of `src/game/behaviors/coffin.inc.c`.
//!
//! The spawner lays out six coffins (two rows of three); the odd ones are the animated ones: each stands up when Mario
//! walks in front of it, shakes, then lies back down. Every coffin carries Mario as a surface object
//! (`bbh_seg7_collision_coffin`). The room culling of `bhv_init_room` is not modelled: the coffins stay.
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, run_platform};
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;
use crate::state::*;

const KEY: &str = "g2/bbh_seg7_collision_coffin";
pub const COFFIN_MODEL: &str = "levels/bbh/geo_bbh_000658.glb";

/// `coffinRelativePos`: (left, forward) of the six coffins.
const POS: [(f64, f64); 6] = [(412.0, -150.0), (762.0, -150.0), (1112.0, -150.0), (412.0, 150.0), (762.0, 150.0), (1112.0, 150.0)];

pub fn init_spawner(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_coffin_spawner_loop`: the coffins load once.
pub fn spawner(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if m.objects[id].action != 0 {
            return;
        }
        let (pos, yaw) = (m.objects[id].pos, m.objects[id].move_yaw);
        for (i, (left, forward)) in POS.iter().enumerate() {
            let c = spawn_child(m, id, K::Coffin, Some(COFFIN_MODEL), (i & 1) as f64);
            let o = &mut m.objects[c];
            o.pos = [pos[0] + left * coss(yaw) + forward * sins(yaw), pos[1], pos[2] - left * sins(yaw) + forward * coss(yaw)];
            o.set_var("param2", (i & 1) as f64);
            // Coffins on the far side of the room face the other way.
            o.face_yaw = if *forward > 0.0 { 0x8000 as f64 } else { yaw };
            o.move_yaw = o.face_yaw;
            o.visible = true;
        }
        m.objects[id].action = 1;
    });
}

pub fn init_coffin(o: &mut SimObject) {
    o.visible = true;
}

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

/// `obj_face_pitch_approach(target, delta)`.
fn face_pitch_approach(o: &mut SimObject, target: f64, delta: f64) -> bool {
    let p = approach_s16_symmetric(o.var("pitch"), target, delta);
    o.set_var("pitch", p);
    s16(p) == s16(target)
}

/// `bhv_coffin_loop`.
pub fn coffin(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        let (mario, squished) = (m.pos, m.action == ACT_SQUISHED);
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if o.var("param2") != 0.0 {
                    if o.var("pitch") != 0.0 {
                        // Lay down.
                        let v = approach_s16_symmetric(o.var("avPitch"), -2000.0, 200.0);
                        o.set_var("avPitch", v);
                        face_pitch_approach(o, 0.0, -v);
                        o.timer = 0.0;
                    } else {
                        let (c, s) = (coss(o.face_yaw), sins(o.face_yaw));
                        let (dx, dz) = (mario[0] - o.pos[0], mario[2] - o.pos[2]);
                        let forwards = dx * c + dz * s;
                        let sideways = dz * c - dx * s;
                        if o.timer > 60.0 && (o.distance_to_mario > 100.0 || squished) && mario[1] - o.pos[1] < 200.0 && forwards.abs() < 140.0 && sideways < 150.0 && sideways > -450.0 {
                            o.action = 1;
                        }
                        o.set_var("avPitch", 0.0);
                    }
                }
            }
            _ => {
                if o.var("pitch") != 0x4000 as f64 {
                    let v = approach_s16_symmetric(o.var("avPitch"), 1000.0, 200.0);
                    o.set_var("avPitch", v);
                    face_pitch_approach(o, 0x4000 as f64, v);
                } else {
                    if o.timer > 60.0 {
                        o.action = 0;
                        o.set_var("roll", 0.0);
                    } else if o.timer > 30.0 {
                        // Shake while standing (`gGlobalTimer % 2` approximated by the object timer).
                        o.set_var("roll", 400.0 * (o.timer % 2.0) - 200.0);
                    }
                    o.set_var("avPitch", 0.0);
                }
            }
        }
        load_object_collision_model(m, id, KEY, 1000.0);
    });
}
