//! Tiny-Huge Island: the huge island's plug and the tiny island's top that drains the pool
//! (thi_top.inc.c) and the Fire Piranha Plants (fire_piranha_plant.inc.c, flame.inc.c).
//!
//! The size change itself is not an object: the huge and tiny islands are two areas of the
//! level (area 1 and area 2) joined by warp pipes, which the course loader links in place.
use super::dyn_collision::*;
use super::interact::*;
use super::lll::{bparam2, hitbox, RED_FLAME};
use super::obj::*;
use super::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

/// object_list_processor.c `gTHIWaterDrained & 1` (reset per level load; here per process).
static WATER_DRAINED: AtomicBool = AtomicBool::new(false);

pub fn water_drained() -> bool {
    WATER_DRAINED.load(Ordering::Relaxed)
}

/// Test/loader hook: forget the drained pool (`clear_objects` sets `gTHIWaterDrained = 0`).
pub fn reset_water_drained() {
    WATER_DRAINED.store(false, Ordering::Relaxed);
}

/// `gEnvironmentRegions[6 * index + 6]`: the height of the `index`-th water box.
pub fn region_y(m: &MarioRuntime, index: usize) -> Option<f64> {
    m.world.water_boxes.get(index).map(|b| b.5)
}

fn set_region_y(m: &mut MarioRuntime, index: usize, y: f64) {
    if m.world.water_boxes.get(index).is_some_and(|b| b.5 != y) {
        Arc::make_mut(&mut m.world).water_boxes[index].5 = y;
    }
}

const K_TOP_TRAP: &str = "g2/thi_seg7_collision_top_trap";

pub fn init_plain(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_thi_huge_island_top_loop`: the plug over the huge island's pool. Once the tiny island
/// has drained, the plug disappears and the pool (`gEnvironmentRegions[18]`) falls to 3000.
pub fn huge_island_top(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if water_drained() {
            if m.objects[id].timer == 0.0 {
                set_region_y(m, 2, 3000.0);
            }
            m.objects[id].visible = false;
            unload_object_collision(m, id);
        } else {
            m.objects[id].visible = true;
            load_object_collision_model(m, id, K_TOP_TRAP, 1000.0);
        }
    });
}

/// `bhv_thi_tiny_island_top_loop`: ground-pound the top of the tiny island and the pool drains
/// by one unit for 50 frames, then settles at 700.
pub fn tiny_island_top(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if !water_drained() {
            let (action, dist, timer) = {
                let o = &m.objects[id];
                (o.action, o.distance_to_mario, o.timer)
            };
            if action == 0 {
                if dist < 500.0 && m.action == ACT_GROUND_POUND_LAND {
                    let o = &mut m.objects[id];
                    o.action += 1;
                    o.visible = false;
                }
            } else if timer < 50.0 {
                if let Some(y) = region_y(m, 2) {
                    set_region_y(m, 2, y - 1.0);
                }
            } else {
                WATER_DRAINED.store(true, Ordering::Relaxed);
                m.objects[id].action += 1;
            }
        } else {
            if m.objects[id].timer == 0.0 {
                set_region_y(m, 2, 700.0);
            }
            m.objects[id].visible = false;
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Fire Piranha Plant
// ---------------------------------------------------------------------------------------------

/// `D_80331B5C`: neutral scale by `oBhvParams2ndByte`.
const NEUTRAL_SCALE: [f64; 2] = [0.5, 2.0];

/// `sFirePiranhaPlantHitbox`.
const PLANT_HITBOX: ObjectHitbox =
    ObjectHitbox { interact_type: INTERACT_BOUNCE_TOP, down_offset: 0.0, damage_or_coin_value: 2.0, health: 0.0, num_loot_coins: 1.0, radius: 80.0, height: 160.0, hurtbox_radius: 50.0, hurtbox_height: 150.0 };

/// `bhv_fire_piranha_plant_init`.
pub fn init_piranha_plant(o: &mut SimObject) {
    let bp = bparam2(o) as usize;
    o.set_var("neutral", NEUTRAL_SCALE[bp.min(1)]);
    o.hitbox = PLANT_HITBOX;
    o.visible = false;
    if bp != 0 {
        o.set_var("health", 1.0);
        o.hitbox.num_loot_coins = if (o.param as u32) & 0xFF00 != 0 { 0.0 } else { 2.0 };
    }
    o.set_var("scale", 0.0);
    o.scale = 0.0;
    // `HIDE()` / `DROP_TO_FLOOR()` run on the first script frame.
}

/// object_helpers.c `approach_f32_ptr`: true when the value reached `target`.
fn approach_f32_ptr(value: &mut f64, target: f64, inc: f64) -> bool {
    if *value > target {
        *value -= inc;
        if *value < target {
            *value = target;
        }
    } else {
        *value += inc;
        if *value > target {
            *value = target;
        }
    }
    *value == target
}

/// `sNumActiveFirePiranhaPlants` is tracked on every plant's group leader: the first plant.
fn active_plants(m: &MarioRuntime) -> usize {
    m.objects.objects.iter().filter(|o| o.behavior == ObjectBehaviorKind::FirePiranhaPlant && o.active && o.var("active") != 0.0).count()
}

/// `bhv_fire_piranha_plant_update` (the attack check is the bounce-top interaction in
/// [`super::interact::bounce_top`], which sets `INT_STATUS_WAS_ATTACKED`).
pub fn piranha_plant(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, F_FACE_YAW, |m, id| {
        if first_frame(&mut m.objects[id]) {
            drop_to_floor(&mut m.objects[id], &world);
            m.objects[id].home = m.objects[id].pos;
        }
        let active_total = active_plants(m);
        let o = &mut m.objects[id];
        let neutral = o.var("neutral");
        o.scale = o.var("scale");
        let angle_to_mario = o.angle_to_mario;
        let dist = o.distance_to_mario;
        let bp = bparam2(o);
        let mut killed_star = false;
        match o.action {
            // fire_piranha_plant_act_hide
            0 => {
                if o.var("spinTimer") != 0.0 {
                    o.move_yaw = s16(o.move_yaw + o.var("spinVel").trunc());
                    let mut v = o.var("spinVel");
                    approach_f32_ptr(&mut v, 0.0, 200.0);
                    o.set_var("spinVel", v);
                    let t = o.var("spinTimer") - 1.0;
                    // cur_obj_check_if_near_animation_end: the death spin runs about ten frames.
                    o.set_var("spinTimer", if o.timer % 4.0 == 3.0 { t } else { o.var("spinTimer") });
                } else {
                    let mut sc = o.var("scale");
                    if approach_f32_ptr(&mut sc, 0.0, 0.04 * neutral) {
                        o.visible = false;
                        o.hitbox.interact_type = 0; // cur_obj_become_intangible
                        if o.var("active") != 0.0 {
                            o.set_var("active", 0.0);
                            if bp != 0 && o.var("health") == 0.0 {
                                killed_star = true;
                            }
                        } else if active_total < 2 && o.timer > 100.0 && dist > 100.0 && dist < 800.0 {
                            o.set_var("active", 1.0);
                            o.visible = true;
                            o.action = 1;
                            o.move_yaw = angle_to_mario;
                        } else {
                            o.visible = false;
                        }
                    }
                    o.set_var("scale", sc);
                }
            }
            // fire_piranha_plant_act_grow
            1 => {
                let mut sc = o.var("scale");
                if approach_f32_ptr(&mut sc, neutral, 0.04 * neutral) {
                    if o.timer > 80.0 {
                        o.action = 0;
                    } else if o.timer < 50.0 {
                        rotate_yaw_toward(o, angle_to_mario, 0x400 as f64);
                    } else if o.timer == 56.0 {
                        // obj_spit_fire(0, 30 * s, 140 * s, 2.5 * s, MODEL_RED_FLAME_SHADOW, 20, 15, 0x1000)
                        let (pos, yaw) = (o.pos, o.move_yaw);
                        o.set_var("scale", sc);
                        spawn_piranha_flame(m, id, pos, yaw, neutral);
                        return;
                    }
                } else if sc > neutral / 2.0 {
                    o.hitbox.interact_type = INTERACT_BOUNCE_TOP;
                }
                o.set_var("scale", sc);
            }
            _ => {}
        }
        if killed_star {
            // `if (++sNumKilledFirePiranhaPlants == 5) spawn_default_star(-6300, -1850, -6300)`
            let n = m.objects.objects.iter().filter(|p| p.behavior == ObjectBehaviorKind::FirePiranhaPlant && p.var("dead") != 0.0).count() + 1;
            if n == 5 {
                // The plant's BPARAM1 is STAR_INDEX_ACT_1: the act 1 star rises from the fifth plant.
                crate::behaviors::r2a::helpers::spawn_default_star(m, id, [-6300.0, -1850.0, -6300.0]);
            }
            // obj_die_if_health_non_positive: `oNumLootCoins` yellow coins (2 for the star plants, 0 with BPARAM3).
            let (pos, loot) = (m.objects[id].pos, m.objects[id].hitbox.num_loot_coins as usize);
            let p = &mut m.objects[id];
            p.set_var("dead", 1.0);
            p.active = false;
            p.visible = false;
            for _ in 0..loot {
                m.objects.spawn_coin(pos, true, 1.0);
            }
            return;
        }
        // obj_check_attacks: a stomp / punch / ground pound starts the death spin.
        let o = &mut m.objects[id];
        if o.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
            o.interact_status = 0;
            let h = o.var("health") - 1.0;
            o.set_var("health", h);
            if h < 0.0 && o.var("active") != 0.0 {
                o.set_var("active", 0.0);
            }
            o.action = 0;
            o.set_var("spinTimer", 10.0);
            o.set_var("spinVel", 8000.0);
            o.hitbox.interact_type = 0;
        }
    });
}

pub fn init_piranha_flame(o: &mut SimObject) {
    o.hitbox = hitbox(INTERACT_FLAME, 30.0, 40.0, 20.0);
    o.billboard = true;
    o.visible = true;
    phys::set_physics(o, 30.0, 0.0, -50.0, 1000.0, 1000.0, 200.0);
}

/// `obj_spit_fire` -> `bhvSmallPiranhaFlame` (`oBhvParams2ndByte = 1`).
fn spawn_piranha_flame(m: &mut MarioRuntime, plant: ObjId, _pos: Vec3, _yaw: f64, neutral: f64) {
    spit_fire(m, plant, 30.0 * neutral, 140.0 * neutral, 2.5 * neutral, 20.0, 15.0, 0x1000 as f64);
}

/// `obj_spit_fire(0, rel_y, rel_z, scale, MODEL_RED_FLAME_SHADOW, start, end, 0x1000)`: the offset follows the spitter's move yaw.
pub fn spit_fire(m: &mut MarioRuntime, parent: ObjId, rel_y: f64, rel_z: f64, scale: f64, start: f64, end: f64, pitch: f64) {
    let (pos, yaw) = (m.objects[parent].pos, m.objects[parent].move_yaw);
    let f = spawn_child(m, parent, ObjectBehaviorKind::SmallPiranhaFlame, Some(RED_FLAME), 1.0);
    let c = &mut m.objects[f];
    c.pos = [pos[0] + rel_z * sins(yaw), pos[1] + rel_y, pos[2] + rel_z * coss(yaw)];
    c.move_yaw = yaw;
    c.scale = scale;
    c.set_var("speed", start);
    c.set_var("endSpeed", end);
    c.set_var("mpitch", pitch);
}

/// `bhv_small_piranha_flame_loop` (`oBhvParams2ndByte != 0`): the fireball flies toward Mario and
/// leaves a trail; its own hitbox stands in for the `bhvFlyguyFlame` trail objects.
pub fn small_piranha_flame(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let o = &mut m.objects[id];
        phys::update_floor_and_walls(o, &world);
        let mut speed = o.var("speed");
        if approach_f32_ptr(&mut speed, o.var("endSpeed"), 0.6) {
            let target = o.angle_to_mario;
            rotate_yaw_toward(o, target, 0x200 as f64);
        }
        o.set_var("speed", speed);
        // obj_compute_vel_from_move_pitch
        let pitch = o.var("mpitch");
        o.forward_vel = speed * coss(pitch);
        o.vel[1] = speed * -sins(pitch);
        phys::move_standard(o, &world, -78.0);
        if o.timer > 90.0 || phys::flags(o) & (phys::MASK_ON_GROUND | phys::MOVE_HIT_WALL) != 0 {
            mark_for_deletion(o);
        }
        o.interact_status = 0;
    });
}
