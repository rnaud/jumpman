//! The castle moat: `bhvWaterLevelPillar` (`src/game/behaviors/water_pillar.inc.c`, the two basement
//! pillars), `bhvMoatGrills` (`moat_grill.inc.c`, the grills over the Vanish Cap hole) and
//! `bhvInvisibleObjectsUnderBridge` (`moat_drainer.inc.c`, the drained moat level, folded into the grill).
//! `SAVE_FLAG_MOAT_DRAINED` lives in the save file: the app publishes it as the `drained` var
//! (`progress::castle_npcs`) and saves it when a pillar reports `drainedNow`.
//!
//! Water levels: `gEnvironmentLevels[0]` / `[2]` drive `gEnvironmentRegions[6]` / `[18]`, i.e. the
//! heights of water boxes 0 and 2 of the area (each box is 6 shorts after the count); the moat drainer
//! sets regions 6 and 12 (boxes 0 and 1 of the castle grounds) to -800.
use super::*;
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, unload_object_collision};
use crate::behaviors::g2::obj::mario_on_platform;
use crate::state::*;
use std::sync::Arc;

pub const PILLAR_COLLISION: &str = "g2/inside_castle_seg7_collision_water_level_pillar";
pub const GRILL_COLLISION: &str = "g2/castle_grounds_seg7_collision_moat_grills";
/// `approach_f32_symmetric(gEnvironmentLevels[n], -2450.0f, 5.0f)`.
pub const DRAINED_BASEMENT_LEVEL: f64 = -2450.0;
/// `gEnvironmentRegions[6] = gEnvironmentRegions[12] = -800` (bhv_invisible_objects_under_bridge_init).
pub const DRAINED_MOAT_LEVEL: f64 = -800.0;

fn water_box_y(m: &MarioRuntime, index: usize) -> Option<f64> {
    m.world.water_boxes.get(index).map(|b| b.5)
}

fn set_water_box_y(m: &mut MarioRuntime, index: usize, y: f64) {
    if water_box_y(m, index).is_some_and(|old| old != y) {
        Arc::make_mut(&mut m.world).water_boxes[index].5 = y;
    }
}

pub fn init_pillar(o: &mut SimObject) {
    o.visible = true;
}

/// bhv_water_level_pillar_loop + load_object_collision_model.
pub fn pillar(id: ObjId, m: &mut MarioRuntime) {
    if !m.objects[id].has_var("drained") {
        // The save file has not been published (headless runs): stay idle and solid.
        load_object_collision_model(m, id, PILLAR_COLLISION, 1000.0);
        return;
    }
    if m.objects[id].var("drained") != 0.0 {
        // water_level_pillar_drained
        if m.objects[id].action_timer == 0.0 {
            m.objects[id].pos[1] -= 80.0;
            set_water_box_y(m, 0, DRAINED_BASEMENT_LEVEL);
            set_water_box_y(m, 2, DRAINED_BASEMENT_LEVEL);
        }
    } else {
        undrained(id, m);
    }
    load_object_collision_model(m, id, PILLAR_COLLISION, 1000.0);
    end(&mut m.objects[id]);
}

/// water_level_pillar_undrained.
fn undrained(id: ObjId, m: &mut MarioRuntime) {
    let other = nearest_of(m, id, K::WaterLevelPillar);
    match m.objects[id].action {
        0 => {
            // cur_obj_is_mario_ground_pounding_platform
            if mario_on_platform(m, id) && m.action == ACT_GROUND_POUND_LAND {
                m.objects[id].action = 1;
            }
        }
        1 => {
            let o = &mut m.objects[id];
            if o.action_timer < 4.0 {
                o.pos[1] -= 20.0;
            } else {
                o.action = 2;
            }
        }
        2 => {
            if other.is_some_and(|p| m.objects[p].action < 2) {
                m.objects[id].action = 3;
            }
        }
        3 => {
            if other.is_some_and(|p| m.objects[p].action > 1) {
                m.objects[id].action = 4;
                // save_file_set_flags(SAVE_FLAG_MOAT_DRAINED) + play_puzzle_jingle
                m.objects[id].set_var("drainedNow", 1.0);
                let pos = m.objects[id].pos;
                m.objects.sound(pos, "MENU_COLLECT_SECRET");
            }
        }
        4 => {
            if m.objects[id].action_timer < 300.0 {
                for b in [2usize, 0] {
                    if let Some(y) = water_box_y(m, b) {
                        set_water_box_y(m, b, approach_f32(y, DRAINED_BASEMENT_LEVEL, 5.0, 5.0).trunc());
                    }
                }
                if m.objects[id].action_timer % 30.0 == 0.0 {
                    let pos = m.objects[id].pos;
                    m.objects.sound(pos, "ENV_WATER_DRAIN");
                }
            } else {
                m.objects[id].action = 5;
            }
        }
        _ => {}
    }
}

pub fn init_grill(o: &mut SimObject) {
    o.visible = true;
}

/// bhv_moat_grills_loop (collision distance 30000) and the drained moat level.
pub fn grill(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("drained") != 0.0 {
        m.objects[id].visible = false;
        unload_object_collision(m, id);
        set_water_box_y(m, 0, DRAINED_MOAT_LEVEL);
        set_water_box_y(m, 1, DRAINED_MOAT_LEVEL);
    } else {
        m.objects[id].visible = true;
        load_object_collision_model(m, id, GRILL_COLLISION, 30000.0);
    }
}
