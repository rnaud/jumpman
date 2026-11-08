//! Dire, Dire Docks ring stars: `bhvMantaRay` ("The Manta Ray's Reward") and `bhvJetStreamRingSpawner`
//! ("Through the Jet Stream"), with the water rings both leave behind. Port of
//! `src/game/behaviors/manta_ray.inc.c` and `water_ring.inc.c`.
//!
//! Collect five rings in a row (each must be the next index, or the count restarts) and the star
//! appears: (-3180, -3600, 120) for the manta ray, (3400, -3200, -500) for the jet stream. As in the
//! decomp the rings test "Mario crossed my plane" with the faulty normal of `water_ring_init`
//! (assuming yaw 0), so a manta ring is not always collected where it looks like it should be.
//! The Jet Stream Ring Spawner is its own ring manager; the Manta Ray's separate `bhvMantaRayRingManager`
//! (two counters) is folded into the ray's vars.
//!
//! Left out: opacity fades, the orange number popups, sounds.
use super::helpers::*;
use crate::behaviors::g1::anim::{self, AnimSpec};
use crate::behaviors::g2::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use ObjectBehaviorKind as K;

/// `sMantaRayTraj` (manta_ray.inc.c).
pub const MANTA_TRAJECTORY: [[f64; 3]; 8] = [
    [-4500.0, -1380.0, -40.0],
    [-4120.0, -2240.0, 740.0],
    [-3280.0, -3080.0, 1040.0],
    [-2240.0, -3320.0, 720.0],
    [-1840.0, -3140.0, -280.0],
    [-2320.0, -2480.0, -1100.0],
    [-3220.0, -1600.0, -1360.0],
    [-4180.0, -1020.0, -1040.0],
];
/// `manta_seg5_anims_05008EB4`, `water_ring_seg6_anims_06013F7C`.
pub const MANTA_ANIMS: [AnimSpec; 1] = [anim_spec(false, 0, 0, 199)];
pub const MANTA_ANIM_NAMES: [&str; 1] = ["manta_seg5_anim_05008CFC"];
pub const RING_ANIMS: [AnimSpec; 1] = [anim_spec(false, 0, 0, 59)];
pub const RING_ANIM_NAMES: [&str; 1] = ["water_ring_seg6_anim_06013F64"];

pub const MANTA_ACT_SPAWN_RINGS: i32 = 0;
pub const MANTA_ACT_NO_RINGS: i32 = 1;
pub const RING_NOT_COLLECTED: i32 = 0;
pub const RING_COLLECTED: i32 = 1;

/// sMantaRayHitbox.
pub const MANTA_HITBOX: ObjectHitbox = hitbox(INTERACT_DAMAGE, 0.0, 0.0, 3.0, 0.0, 210.0, 60.0, 200.0, 50.0);
/// `SET_HITBOX_WITH_OFFSET(75, 20, 20)`, `INTERACT_WATER_RING`, `oDamageOrCoinValue = 2`.
pub const RING_HITBOX: ObjectHitbox = hitbox(super::interact::INTERACT_WATER_RING, 20.0, 2.0, 0.0, 0.0, 75.0, 20.0, 75.0, 20.0);

pub fn init_manta(o: &mut SimObject) {
    o.visible = true;
    o.scale = 2.5;
    o.hitbox = MANTA_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    anim::init(o, 0, MANTA_ANIMS[0]);
}

/// `cur_obj_follow_path` over the manta trajectory (`oPathedPrevWaypointFlags` collapsed into the index).
fn follow_path(o: &mut SimObject) {
    if o.var("pathInit") == 0.0 {
        o.set_var("pathInit", 1.0);
        o.set_var("prevWaypoint", 0.0);
    }
    let last = o.var("prevWaypoint") as usize;
    let target = if last + 1 < MANTA_TRAJECTORY.len() { last + 1 } else { 0 };
    let (l, t) = (MANTA_TRAJECTORY[last], MANTA_TRAJECTORY[target]);
    let p2n = [t[0] - l[0], t[1] - l[1], t[2] - l[2]];
    let o2n = [t[0] - o.pos[0], t[1] - o.pos[1], t[2] - o.pos[2]];
    let xz = (o2n[0] * o2n[0] + o2n[2] * o2n[2]).sqrt();
    o.set_var("targetYaw", atan2s(o2n[2], o2n[0]) as f64);
    o.set_var("targetPitch", atan2s(xz, -o2n[1]) as f64);
    if p2n[0] * o2n[0] + p2n[1] * o2n[1] + p2n[2] * o2n[2] <= 0.0 {
        o.set_var("prevWaypoint", target as f64);
    }
}

/// `manta_ray_move`.
fn manta_move(o: &mut SimObject) {
    follow_path(o);
    o.forward_vel = 10.0;
    o.move_yaw = approach_s16_symmetric(o.move_yaw, o.var("targetYaw"), 0x80 as f64);
    let pitch = approach_s16_symmetric(o.var("movePitch"), o.var("targetPitch"), 0x80 as f64);
    o.set_var("movePitch", pitch);
    // The ray tilts as it turns.
    let mut roll = o.var("faceRoll");
    if to_s16(o.var("targetYaw")) != to_s16(o.move_yaw) {
        roll -= 91.0;
        if roll < -5461.3332 {
            roll = -(0x4000 / 3) as f64;
        }
    } else {
        roll += 91.0;
        if roll > 5461.3332 {
            roll = (0x4000 / 3) as f64;
        }
    }
    o.set_var("faceRoll", roll);
    // cur_obj_set_pos_via_transform
    let (sx, cx) = (sins(pitch), coss(pitch));
    o.pos[0] += cx * sins(o.move_yaw) * o.forward_vel;
    o.pos[1] += -sx * o.forward_vel;
    o.pos[2] += cx * coss(o.move_yaw) * o.forward_vel;
}

fn spawn_ring(m: &mut MarioRuntime, spawner: ObjId, manta: bool) {
    let ring = spawn_child(m, spawner, K::WaterRing, Some("actors/water_ring/water_ring_geo.glb"), 0.0);
    let (spos, syaw, spitch) = {
        let s = &m.objects[spawner];
        (s.pos, s.move_yaw, s.var("movePitch"))
    };
    let index = m.objects[spawner].var("nextRing");
    let r = &mut m.objects[ring];
    r.set_var("manta", manta as i32 as f64);
    r.set_var("ringIndex", index);
    if manta {
        r.face_yaw = syaw;
        r.set_var("facePitch", to_s16(spitch + 0x4000 as f64) as f64);
        r.pos[0] = spos[0] + 200.0 * sins(syaw + 0x8000 as f64);
        r.pos[1] = spos[1] + 10.0 + 200.0 * sins(spitch);
        r.pos[2] = spos[2] + 200.0 * coss(syaw + 0x8000 as f64);
    }
    let mut next = index + 1.0;
    if next > 10000.0 {
        next = 0.0;
    }
    m.objects[spawner].set_var("nextRing", next);
}

/// `bhv_manta_ray_loop`.
pub fn manta(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        match m.objects[id].action {
            MANTA_ACT_SPAWN_RINGS => {
                manta_move(&mut m.objects[id]);
                // manta_ray_act_spawn_ring
                if m.objects[id].timer == 300.0 {
                    m.objects[id].timer = 0.0;
                }
                let t = m.objects[id].timer as i32;
                if matches!(t, 0 | 50 | 150 | 200 | 250) {
                    spawn_ring(m, id, true);
                }
                if m.objects[id].var("collected") == 5.0 {
                    m.objects[id].action = MANTA_ACT_NO_RINGS;
                    spawn_default_star(m, id, [-3180.0, -3600.0, 120.0]);
                }
            }
            _ => manta_move(&mut m.objects[id]),
        }
        let o = &mut m.objects[id];
        o.face_yaw = o.move_yaw;
        o.set_var("facePitch", o.var("movePitch"));
        o.interact_status = 0;
        advance_anim(o);
    });
}

pub fn init_spawner(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_jet_stream_ring_spawner_loop`.
pub fn jet_stream_spawner(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if m.objects[id].action == 0 {
            if m.objects[id].timer == 300.0 {
                m.objects[id].timer = 0.0;
            }
            let t = m.objects[id].timer as i32;
            if matches!(t, 0 | 50 | 150 | 200 | 250) {
                spawn_ring(m, id, false);
            }
            if m.objects[id].var("collected") == 5.0 {
                m.objects[id].action = 1;
                spawn_default_star(m, id, [3400.0, -3200.0, -500.0]);
            }
        }
    });
}

pub fn init_ring(o: &mut SimObject) {
    o.visible = true;
    o.hitbox = RING_HITBOX;
    o.set_var("hbInit", 1.0);
    o.set_var("realInteract", super::interact::INTERACT_WATER_RING as f64);
    anim::init(o, 0, RING_ANIMS[0]);
}

/// `water_ring_calc_mario_dist`.
fn mario_dist_in_front(o: &SimObject, mario: Vec3) -> f64 {
    let d = [o.pos[0] - mario[0], o.pos[1] - (mario[1] + 80.0), o.pos[2] - mario[2]];
    d[0] * o.var("nx") + d[1] * o.var("ny") + d[2] * o.var("nz")
}

fn set_scale(o: &mut SimObject, avg: f64) {
    o.set_var("scaleX", sins(o.var("phaseX")) * 0.1 + avg);
    o.set_var("scaleY", sins(o.var("phaseY")) * 0.5 + avg);
    o.set_var("scale", avg);
    for k in ["phaseX", "phaseY", "phaseZ"] {
        o.set_var(k, o.var(k) + 0x1700 as f64);
    }
    o.scale = o.var("scaleX");
}

/// `bhv_jet_stream_water_ring_loop` / `bhv_manta_ray_water_ring_loop` (+ `water_ring_init` on the first frame).
pub fn ring(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let mario = m.pos;
        if first_frame(&mut m.objects[id]) {
            let o = &mut m.objects[id];
            o.set_var("phaseX", (random_float() * 4096.0).trunc() + 0x1000 as f64);
            o.set_var("phaseY", (random_float() * 4096.0).trunc() + 0x1000 as f64);
            o.set_var("phaseZ", (random_float() * 4096.0).trunc() + 0x1000 as f64);
            let (pitch, roll) = (o.var("facePitch"), o.var("faceRoll"));
            o.set_var("nx", coss(pitch) * sins(roll) * -1.0);
            o.set_var("ny", coss(pitch) * coss(roll));
            o.set_var("nz", sins(pitch));
            let d = mario_dist_in_front(o, mario);
            o.set_var("distInFront", d);
            if o.var("manta") == 0.0 {
                o.set_var("facePitch", 0x8000 as f64); // bhv_jet_stream_water_ring_init
            }
        }
        let Some(spawner) = m.objects[id].link else { return };
        match m.objects[id].action {
            RING_NOT_COLLECTED => {
                let manta = m.objects[id].var("manta") != 0.0;
                let timer = m.objects[id].timer;
                let avg = if manta { (timer / 50.0 * 1.3 + 0.1).min(1.3) } else { timer / 225.0 * 3.0 + 0.5 };
                let limit = if manta { 150.0 } else { 225.0 };
                if timer > limit {
                    // the opacity runs out 2 per frame from 150 (manta) / 70 (jet stream)
                    let start = if manta { 150.0 } else { 70.0 };
                    if (timer - limit) * 2.0 >= start - 2.0 {
                        mark_for_deletion(&mut m.objects[id]);
                        return;
                    }
                }
                // water_ring_check_collection
                let dist = mario_dist_in_front(&m.objects[id], mario);
                let close = {
                    let o = &m.objects[id];
                    let d = [o.pos[0] - mario[0], o.pos[1] - (mario[1] + 80.0), o.pos[2] - mario[2]];
                    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() < (avg + 0.2) * 120.0
                };
                if close {
                    if m.objects[id].var("distInFront") * dist < 0.0 {
                        let index = m.objects[id].var("ringIndex");
                        let last = m.objects[spawner].var("lastRing");
                        let collected = m.objects[spawner].var("collected");
                        if index == last + 1.0 || collected == 0.0 {
                            m.objects[spawner].set_var("collected", collected + 1.0);
                            m.objects[spawner].set_var("lastRing", index);
                        } else {
                            m.objects[spawner].set_var("collected", 0.0);
                        }
                        m.objects[id].action = RING_COLLECTED;
                    }
                }
                let o = &mut m.objects[id];
                o.set_var("distInFront", dist);
                set_scale(o, avg);
                if !manta {
                    o.pos[1] += 10.0;
                    o.face_yaw = to_s16(o.face_yaw + 0x100 as f64) as f64;
                }
                o.set_var("avgScale", avg);
            }
            _ => {
                let o = &mut m.objects[id];
                let avg = o.timer * 0.2 + o.var("avgScale");
                if o.timer > 20.0 {
                    mark_for_deletion(o);
                    return;
                }
                set_scale(o, avg);
            }
        }
    });
}
