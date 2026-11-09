//! Tumbling bridges (`bhvTumblingBridge` / `bhvBBHTumblingBridge`: Whomp's Fortress, Big Boo's Haunt, Bowser in the Fire
//! Sea). Port of `src/game/behaviors/tumbling_bridge.inc.c`.
//!
//! The spawner (the whole-bridge model) waits until Mario is within 1000, then replaces itself with nine solid
//! platforms (`bhvTumblingBridgePlatform`, collision 300 units around Mario). Stepping on a platform makes it
//! wobble for 5 frames and then fall away tumbling; once Mario is 1200 units away the spawner shows the intact bridge
//! again and the platforms are deleted. The Lethal Lava Land variant is not placed in any level.
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, unload_object_collision};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

/// One row of `sTumblingBridgeData`: sections, relative start (x for the Fire Sea one, z otherwise), width, part model,
/// collision key (`g2/<symbol>`).
struct Row {
    sections: usize,
    start: f64,
    width: f64,
    model: &'static str,
    collision: &'static str,
    along_x: bool,
}

const ROWS: [Row; 4] = [
    Row { sections: 9, start: -512.0, width: 128.0, model: "levels/wf/wf_geo_000AB0.glb", collision: "g2/wf_seg7_collision_tumbling_bridge", along_x: false },
    Row { sections: 9, start: -412.0, width: 103.0, model: "levels/bbh/geo_bbh_0005F8.glb", collision: "g2/bbh_seg7_collision_07026B1C", along_x: false },
    Row { sections: 9, start: -512.0, width: 128.0, model: "", collision: "g2/lll_seg7_collision_0701D21C", along_x: false },
    Row { sections: 9, start: -512.0, width: 128.0, model: "levels/bitfs/bitfs_geo_0006D8.glb", collision: "g2/bitfs_seg7_collision_07015288", along_x: true },
];

/// `TUMBLING_BRIDGE_BP_*` of the spawner (`oBhvParams2ndByte`).
fn bridge_id(o: &SimObject) -> usize {
    (((o.param as u32) >> 16) & 0xFF).min(3) as usize
}

pub fn init_bridge(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_tumbling_bridge_loop` (`tumbling_bridge_act_0..3`).
pub fn bridge(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| match m.objects[id].action {
        0 => {
            if m.objects[id].distance_to_mario < 1000.0 {
                m.objects[id].action = 1;
            }
        }
        1 => {
            let which = bridge_id(&m.objects[id]);
            let row = &ROWS[which];
            for i in 0..row.sections {
                let offset = row.start + row.width * i as f64;
                let c = spawn_child(m, id, K::TumblingBridgePlatform, Some(row.model), 0.0);
                let (px, pz) = (m.objects[id].pos[0], m.objects[id].pos[2]);
                let (yaw, py) = (m.objects[id].move_yaw, m.objects[id].pos[1]);
                // spawn_object_relative: the offset turns with the parent's yaw.
                let (dx, dz) = if row.along_x { (offset, 0.0) } else { (0.0, offset) };
                let o = &mut m.objects[c];
                o.pos = [px + dx * coss(yaw) + dz * sins(yaw), py, pz - dx * sins(yaw) + dz * coss(yaw)];
                o.set_var("bridge", which as f64);
                o.visible = true;
            }
            m.objects[id].action = 2;
        }
        2 => {
            // cur_obj_hide: the intact model is replaced by its platforms.
            let o = &mut m.objects[id];
            o.visible = false;
            if o.distance_to_mario > 1200.0 {
                o.action = 3;
                o.visible = true;
            }
        }
        _ => {
            let o = &mut m.objects[id];
            o.visible = true;
            o.action = 0;
        }
    });
}

pub fn init_platform(o: &mut SimObject) {
    o.visible = true;
    phys::set_physics(o, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

/// `bhv_tumbling_bridge_platform_loop`.
pub fn platform(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let parent = m.objects[id].link;
    let parent_resets = parent.is_some_and(|p| m.objects[p].action == 3);
    crate::behaviors::g2::dyn_collision::run_platform(m, id, F_FACE_YAW, |m, id, riding| {
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if riding {
                    o.action = 1;
                    let sign = if random_u16() >= 0x7FFF as f64 { 1.0 } else { -1.0 };
                    o.set_var("rollAccel", sign * 0x80 as f64);
                }
            }
            1 => {
                phys::update_floor_height(o, &world);
                if o.timer > 5.0 {
                    o.action = 2;
                }
            }
            2 => {
                let mut pitch_vel = o.var("avPitch");
                let mut roll_vel = o.var("avRoll");
                if pitch_vel < 0x400 as f64 {
                    pitch_vel += 0x80 as f64;
                }
                if roll_vel > -0x400 as f64 && roll_vel < 0x400 as f64 {
                    roll_vel += o.var("rollAccel");
                }
                o.set_var("avPitch", pitch_vel);
                o.set_var("avRoll", roll_vel);
                // cur_obj_rotate_face_angle_using_vel, then cur_obj_move_using_fvel_and_gravity(-3).
                o.set_var("pitch", to_s16(o.var("pitch") + pitch_vel) as f64);
                o.set_var("roll", to_s16(o.var("roll") + roll_vel) as f64);
                cur_obj_move_using_fvel_and_gravity(o, -3.0);
                if o.pos[1] < o.var("floorH") - 300.0 {
                    o.action = 3;
                }
            }
            _ => {}
        }
        let key = ROWS[o.var("bridge") as usize].collision;
        load_object_collision_model(m, id, key, 300.0);
    });
    if parent_resets {
        unload_object_collision(m, id);
        mark_for_deletion(&mut m.objects[id]);
    }
}
