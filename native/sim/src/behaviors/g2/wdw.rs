//! Wet-Dry World: the changing water level (wdw_water_level.inc.c), arrow lifts
//! (arrow_lift.inc.c), floating platforms (floating_platform.inc.c), the express elevator
//! (express_elevator.inc.c), the town's rotating platform (rotating_platform.inc.c) and the
//! purple switch with its hidden platforms (purple_switch.inc.c, switch_hidden_objects.inc.c).
use super::dyn_collision::*;
use super::lll::bparam2;
use super::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use std::sync::Arc;

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

// ---------------------------------------------------------------------------------------------
// Water level
// ---------------------------------------------------------------------------------------------

/// Index of the water box `gEnvironmentRegions[6]` stands for: the first surface box (region 0).
fn region0(m: &MarioRuntime) -> Option<usize> {
    m.world.water_boxes.iter().position(|b| b.0 < 50.0)
}

/// Current height of region 0 (`gEnvironmentRegions[6]`).
pub fn region_height(m: &MarioRuntime) -> Option<f64> {
    region0(m).map(|i| m.world.water_boxes[i].5)
}

fn set_region_height(m: &mut MarioRuntime, y: f64) {
    if let Some(i) = region0(m) {
        if m.world.water_boxes[i].5 != y {
            Arc::make_mut(&mut m.world).water_boxes[i].5 = y;
        }
    }
}

pub fn init_hidden(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_init_changing_water_level_loop` (bhvInitializeChangingWaterLevel). `vars["lv"]` is
/// `gEnvironmentLevels[0]`, `vars["changing"]` is `gWDWWaterLevelChanging`.
pub fn water_level_init(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let height = region_height(m);
        let o = &mut m.objects[id];
        if o.action == 0 {
            if height.is_some() {
                o.action += 1;
            }
        } else if o.timer < 10.0 {
            if let Some(h) = height {
                o.set_var("lv", h);
            }
        } else {
            let f4 = o.var("f4");
            let level = (o.var("lv") + sins(f4) * 20.0).trunc();
            o.set_var("f4", f4 + 0x200 as f64);
            set_region_height(m, level);
        }
    });
}

pub fn init_water_diamond(o: &mut SimObject) {
    o.hitbox = ObjectHitbox { interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 70.0, height: 30.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 };
    o.visible = true;
}

/// object_helpers.c `approach_f32_symmetric`.
fn approach_f32_symmetric(value: f64, target: f64, increment: f64) -> f64 {
    let dist = target - value;
    if dist >= 0.0 {
        if dist > increment { value + increment } else { target }
    } else if dist < -increment {
        value - increment
    } else {
        target
    }
}

/// `bhv_water_level_diamond_loop`.
pub fn water_diamond(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let Some(controller) = m.objects.find_by_behavior(ObjectBehaviorKind::WaterLevelInit) else { return };
        if region_height(m).is_none() {
            return;
        }
        let touching = mario_overlaps_object(m, &m.objects[id]);
        let changing = m.objects[controller].var("changing") != 0.0;
        match m.objects[id].action {
            0 => {
                let o = &mut m.objects[id];
                o.face_yaw = 0.0;
                o.set_var("target", o.pos[1].trunc());
                if o.timer > 10.0 {
                    o.action += 1;
                }
            }
            1 => {
                if touching && !changing {
                    m.objects[id].action += 1;
                    m.objects[controller].set_var("changing", 1.0);
                }
            }
            2 => {
                let target = m.objects[id].var("target");
                let lv = approach_f32_symmetric(m.objects[controller].var("lv"), target, 10.0).trunc();
                m.objects[controller].set_var("lv", lv);
                let o = &mut m.objects[id];
                o.set_var("avYaw", 0.0);
                if lv == target {
                    if s16(o.face_yaw) == 0.0 {
                        o.action += 1;
                    } else {
                        o.set_var("avYaw", 0x800 as f64);
                    }
                } else {
                    o.set_var("avYaw", 0x800 as f64);
                }
            }
            3 => {
                if !touching {
                    m.objects[controller].set_var("changing", 0.0);
                    let o = &mut m.objects[id];
                    o.action = 1;
                    o.set_var("avYaw", 0.0);
                }
            }
            _ => {}
        }
        let o = &mut m.objects[id];
        o.face_yaw = s16(o.face_yaw + o.var("avYaw"));
    });
}

// ---------------------------------------------------------------------------------------------
// Arrow lifts, floating platforms, express elevator, rotating platform
// ---------------------------------------------------------------------------------------------

const K_ARROW_LIFT: &str = "g2/wdw_seg7_collision_arrow_lift";
const K_SQUARE_FLOAT: &str = "g2/wdw_seg7_collision_square_floating_platform";
const K_RECT_FLOAT: &str = "g2/wdw_seg7_collision_rect_floating_platform";
const K_EXPRESS: &str = "g2/wdw_seg7_collision_express_elevator_platform";
const K_WDW_ROTATING: &str = "g2/wdw_seg7_collision_070186B4";

pub fn init_plain(o: &mut SimObject) {
    o.visible = true;
}

/// `arrow_lift_move_away` / `arrow_lift_move_back`; true when the move is finished.
fn arrow_lift_move(o: &mut SimObject, away: bool) -> bool {
    o.move_yaw = if away { s16(o.face_yaw - 0x4000 as f64) } else { s16(o.face_yaw + 0x4000 as f64) };
    o.vel[1] = 0.0;
    o.forward_vel = 12.0;
    let mut d = o.var("disp") + if away { 12.0 } else { -12.0 };
    let mut done = false;
    if (away && d > 384.0) || (!away && d < 0.0) {
        o.forward_vel = 0.0;
        d = if away { 384.0 } else { 0.0 };
        done = true;
    }
    o.set_var("disp", d);
    // obj_move_xyz_using_fvel_and_yaw
    o.vel[0] = o.forward_vel * sins(o.move_yaw);
    o.vel[2] = o.forward_vel * coss(o.move_yaw);
    o.pos[0] += o.vel[0];
    o.pos[1] += o.vel[1];
    o.pos[2] += o.vel[2];
    done
}

/// `bhv_arrow_lift_loop`.
pub fn arrow_lift(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if o.timer > 60.0 && riding {
                    o.action = 1;
                }
            }
            1 => {
                if arrow_lift_move(o, true) {
                    o.action = 2;
                }
            }
            2 => {
                if o.timer > 60.0 && arrow_lift_move(o, false) {
                    o.action = 0;
                }
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_ARROW_LIFT, 1000.0);
    });
}

/// `bhvWDWSquareFloatingPlatform` / `bhvWDWRectangularFloatingPlatform`: `oFloatingPlatformUnkFC = 64`.
pub fn init_floating_platform(o: &mut SimObject) {
    o.visible = true;
    o.set_var("fc", 64.0);
}

fn floating_platform(id: ObjId, m: &mut MarioRuntime, key: &str) {
    run_platform(m, id, F_FACE_YAW, |m, id, riding| {
        let (px, pz) = (m.pos[0], m.pos[2]);
        let (water, floor) = {
            let o = &m.objects[id];
            (m.world.water_level(o.pos[0], o.pos[2]), m.world.find_floor(o.pos[0], o.pos[1], o.pos[2]).height)
        };
        let o = &mut m.objects[id];
        // floating_platform_find_home_y
        let fc = o.var("fc");
        if water > floor + fc {
            o.set_var("f4", 0.0);
            o.home[1] = water + fc;
        } else {
            o.set_var("f4", 1.0);
            o.home[1] = floor + fc;
        }
        o.action = if o.var("f4") == 0.0 { 0 } else { 1 };
        if o.action == 0 {
            // floating_platform_act_0
            let yaw = o.move_yaw;
            let sp6 = s16(((px - o.pos[0]) * coss(-yaw) + (pz - o.pos[2]) * sins(-yaw)).trunc());
            let sp4 = s16(((pz - o.pos[2]) * coss(-yaw) - (px - o.pos[0]) * sins(-yaw)).trunc());
            let mut f8 = o.var("f8");
            if riding {
                o.set_var("pitch", sp4 * 2.0);
                o.set_var("roll", -sp6 * 2.0);
                o.vel[1] -= 1.0;
                if o.vel[1] < 0.0 {
                    o.vel[1] = 0.0;
                }
                f8 += o.vel[1];
                if f8 > 90.0 {
                    f8 = 90.0;
                }
            } else {
                o.set_var("pitch", (o.var("pitch") / 2.0).trunc());
                o.set_var("roll", (o.var("roll") / 2.0).trunc());
                f8 -= 5.0;
                o.vel[1] = 10.0;
                if f8 < 0.0 {
                    f8 = 0.0;
                }
            }
            o.set_var("f8", f8);
            let f100 = o.var("f100");
            o.pos[1] = o.home[1] - 64.0 - f8 + sins(f100 * 0x800 as f64) * 10.0;
            let next = f100 + 1.0;
            o.set_var("f100", if next == 32.0 { 0.0 } else { next });
        } else {
            o.pos[1] = o.home[1];
        }
        load_object_collision_model(m, id, key, 1000.0);
    });
}

pub fn square_floating_platform(id: ObjId, m: &mut MarioRuntime) {
    floating_platform(id, m, K_SQUARE_FLOAT);
}

pub fn rect_floating_platform(id: ObjId, m: &mut MarioRuntime) {
    floating_platform(id, m, K_RECT_FLOAT);
}

/// `bhv_wdw_express_elevator_loop`.
pub fn express_elevator(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let o = &mut m.objects[id];
        o.vel[1] = 0.0;
        match o.action {
            0 => {
                if riding {
                    o.action += 1;
                }
            }
            1 => {
                o.vel[1] = -20.0;
                o.pos[1] += o.vel[1];
                if o.timer > 132.0 {
                    o.action += 1;
                }
            }
            2 => {
                if o.timer > 110.0 {
                    o.action += 1;
                }
            }
            3 => {
                o.vel[1] = 10.0;
                o.pos[1] += o.vel[1];
                if o.pos[1] >= o.home[1] {
                    o.pos[1] = o.home[1];
                    o.action += 1;
                }
            }
            _ => {
                if !riding {
                    o.action = 0;
                }
            }
        }
        load_object_collision_model(m, id, K_EXPRESS, 1000.0);
    });
}

/// bhvWDWExpressElevatorPlatform: only the collision model.
pub fn express_elevator_platform(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        load_object_collision_model(m, id, K_EXPRESS, 1000.0);
    });
}

/// `bhv_rotating_platform_loop` for `ROTATING_PLATFORM_BP_WDW`: scale 1.5, collision
/// `wdw_seg7_collision_070186B4`, `oAngleVelYaw = (s8) bhvParam1stByte << 4`.
pub fn rotating_platform(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        let o = &mut m.objects[id];
        if o.timer == 0.0 {
            o.scale = 150.0 * 0.01;
        }
        let first = (o.param as u32 >> 24) as u8 as i8 as f64;
        let av = first * 16.0;
        o.set_var("avYaw", av);
        o.face_yaw = s16(o.face_yaw + av);
        load_object_collision_model(m, id, K_WDW_ROTATING, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Purple switch with hidden objects
// ---------------------------------------------------------------------------------------------

const K_PURPLE_SWITCH: &str = "g2/purple_switch_seg8_collision_0800C7A8";
const K_HIDDEN_PLATFORM: &str = "g2/wdw_seg7_collision_07018528";

pub fn init_floor_switch_hidden(o: &mut SimObject) {
    o.visible = true;
    o.scale = 1.5;
    o.set_var("sy", 1.5);
}

/// `cur_obj_scale_over_time(2, 3, from, to)` on the Y axis.
fn scale_y_over_time(o: &mut SimObject, from: f64, to: f64) {
    o.set_var("sy", from + (to - from) * (o.timer / 3.0));
}

/// `bhv_purple_switch_loop` with `oBhvParams2ndByte = 2` (bhvFloorSwitchHiddenObjects).
pub fn floor_switch_hidden(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let lateral = {
            let o = &m.objects[id];
            ((m.pos[0] - o.pos[0]).powi(2) + (m.pos[2] - o.pos[2]).powi(2)).sqrt()
        };
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                o.scale = 1.5;
                o.set_var("sy", 1.5);
                if riding && lateral < 127.5 {
                    o.action = 1;
                }
            }
            1 => {
                scale_y_over_time(o, 1.5, 0.2);
                if o.timer == 3.0 {
                    o.action = 2;
                }
            }
            2 => {
                // oBhvParams2ndByte == 2: stays pressed for 400 frames
                if o.timer > 400.0 {
                    o.action = 4;
                }
            }
            3 => {
                scale_y_over_time(o, 0.2, 1.5);
                if o.timer == 3.0 {
                    o.action = 0;
                }
            }
            4 => {
                if !riding {
                    o.action = 3;
                }
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_PURPLE_SWITCH, 1000.0);
    });
}

pub fn init_hidden_object(o: &mut SimObject) {
    o.visible = false;
}

/// `hidden_wdw_platform_actions` (HIDDEN_OBJECT_BP_WDW_PLATFORM): appears while the switch ticks,
/// blinks for the last 20 frames of its 360 and goes away.
pub fn hidden_object(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        let switch_action = m.objects.find_by_behavior(ObjectBehaviorKind::FloorSwitchHidden).map(|s| m.objects[s].action);
        let o = &mut m.objects[id];
        let hidden_platform = bparam2(o) == 1;
        if !hidden_platform {
            return;
        }
        if o.action == 0 {
            o.visible = false;
            if switch_action == Some(2) {
                o.action += 1;
                o.visible = true;
            }
            unload_object_collision(m, id);
        } else {
            let t = o.timer;
            if t >= 360.0 {
                let blink = t - 360.0;
                if (blink as i64) % 2 != 0 {
                    o.visible = false;
                    if (blink / 2.0).trunc() > 20.0 {
                        o.action = 0;
                    }
                } else {
                    o.visible = true;
                }
            }
            load_object_collision_model(m, id, K_HIDDEN_PLATFORM, 300.0);
        }
    });
}
