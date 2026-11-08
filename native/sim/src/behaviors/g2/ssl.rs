//! Shifting Sand Land objects: pyramid top / elevator / moving walls (pyramid_top.inc.c,
//! pyramid_elevator.inc.c, pyramid_wall.inc.c), Tox Boxes (tox_box.inc.c), Grindel and
//! Thwomp family (thwomp.inc.c, horizontal_grindel.inc.c) and the Spindel (spindel.inc.c).
use super::dyn_collision::*;
use super::lll::bparam2;
use super::obj::*;
use super::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

fn s16(v: f64) -> f64 {
    to_s16(v) as f64
}

pub const DIRT_MODEL: &str = "actors/dirt/dirt_animation_geo.glb";

// ---------------------------------------------------------------------------------------------
// Pyramid top (pyramid_top.inc.c)
// ---------------------------------------------------------------------------------------------

const K_PYRAMID_TOP: &str = "g2/ssl_seg7_collision_pyramid_top";

pub fn init_plain(o: &mut SimObject) {
    o.visible = true;
}

/// `bhv_pyramid_top_init`: the four pillar touch detectors, at absolute positions.
fn pyramid_top_spawn_detectors(m: &mut MarioRuntime, id: ObjId) {
    for (x, y, z) in [(1789.0, 1024.0, 764.0), (1789.0, 896.0, -2579.0), (-5883.0, 1024.0, -2579.0), (-5883.0, 1024.0, 764.0)] {
        let d = spawn_child(m, id, ObjectBehaviorKind::PyramidPillarDetector, None, 0.0);
        let o = &mut m.objects[d];
        o.pos = [x, y, z];
        o.move_yaw = 0.0;
        o.face_yaw = 0.0;
    }
}

/// `bhv_pyramid_top_loop` + `bhv_pyramid_top_spinning` + `bhv_pyramid_top_explode`.
pub fn pyramid_top(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        if first_frame(&mut m.objects[id]) {
            m.objects[id].home = m.objects[id].pos;
            pyramid_top_spawn_detectors(m, id);
        }
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                if o.var("touched") == 4.0 {
                    o.action = 1;
                }
            }
            1 => {
                let t = o.timer;
                o.pos[0] = o.home[0] + sins(t * 0x4000 as f64) * 40.0;
                if t < 60.0 {
                    o.pos[1] = o.home[1] + (sins(t * 0x2000 as f64) * 10.0).abs();
                } else {
                    let mut av = o.var("avYaw") + 0x100 as f64;
                    if av > 0x1800 as f64 {
                        av = 0x1800 as f64;
                        o.vel[1] = 5.0;
                    }
                    o.set_var("avYaw", av);
                    o.face_yaw = s16(o.face_yaw + av);
                    o.pos[1] += o.vel[1];
                }
                if t < 90.0 {
                    let f = spawn_child(m, id, ObjectBehaviorKind::PyramidTopFragment, Some(DIRT_MODEL), 0.0);
                    let c = &mut m.objects[f];
                    c.forward_vel = random_float() * 10.0 + 20.0;
                    c.move_yaw = random_u16();
                    c.scale = 0.8;
                    c.gravity = random_float() + 2.0;
                }
                if m.objects[id].timer == 150.0 {
                    m.objects[id].action = 2;
                }
            }
            2 => {
                for _ in 0..30 {
                    let f = spawn_child(m, id, ObjectBehaviorKind::PyramidTopFragment, Some(DIRT_MODEL), 0.0);
                    let c = &mut m.objects[f];
                    c.forward_vel = random_float() * 50.0 + 80.0;
                    c.vel[1] = random_float() * 80.0 + 20.0;
                    c.move_yaw = random_u16();
                    c.scale = 3.0;
                    c.gravity = random_float() * 2.0 + 5.0;
                }
                mark_for_deletion(&mut m.objects[id]);
                unload_object_collision(m, id);
                return;
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_PYRAMID_TOP, 20000.0);
    });
}

pub fn init_pyramid_fragment(o: &mut SimObject) {
    o.friction = 0.999;
    o.buoyancy = 2.0;
    o.visible = true;
}

/// `bhv_pyramid_top_fragment_loop`: `object_step()`, spin, vanish after 60 frames.
pub fn pyramid_fragment(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let o = &mut m.objects[id];
        o.step_physics(&world, None);
        o.face_yaw = s16(o.face_yaw + 0x1000 as f64);
        let pitch = s16(o.var("pitch") + 0x1000 as f64);
        o.set_var("pitch", pitch);
        if o.timer == 60.0 {
            mark_for_deletion(o);
        }
    });
}

pub fn init_pillar_detector(o: &mut SimObject) {
    o.hitbox = ObjectHitbox { interact_type: 0, down_offset: 0.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 50.0, height: 50.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 };
    o.visible = false;
}

/// `bhv_pyramid_pillar_touch_detector_loop`.
pub fn pillar_detector(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if mario_overlaps_object(m, &m.objects[id]) {
            if let Some(parent) = m.objects[id].link {
                let n = m.objects[parent].var("touched") + 1.0;
                m.objects[parent].set_var("touched", n);
            }
            mark_for_deletion(&mut m.objects[id]);
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Pyramid elevator and moving walls (pyramid_elevator.inc.c, pyramid_wall.inc.c)
// ---------------------------------------------------------------------------------------------

const K_PYRAMID_ELEVATOR: &str = "g2/ssl_seg7_collision_pyramid_elevator";
const K_PYRAMID_WALL: &str = "g2/ssl_seg7_collision_0702808C";

/// `bhv_pyramid_elevator_loop` (the ten trajectory marker balls are not drawn).
pub fn pyramid_elevator(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, riding| {
        let o = &mut m.objects[id];
        if first_frame(o) {
            o.home = o.pos;
        }
        match o.action {
            0 => {
                if riding {
                    o.action = 1;
                }
            }
            1 => {
                o.pos[1] = o.home[1] - sins(o.timer * 0x1000 as f64) * 10.0;
                if o.timer == 8.0 {
                    o.action = 2;
                }
            }
            2 => {
                o.vel[1] = -10.0;
                o.pos[1] += o.vel[1];
                if o.pos[1] < 128.0 {
                    o.pos[1] = 128.0;
                    o.action = 3;
                }
            }
            3 => {
                o.pos[1] = sins(o.timer * 0x1000 as f64) * 10.0 + 128.0;
                if o.timer >= 8.0 {
                    o.vel[1] = 0.0;
                    o.pos[1] = 128.0;
                }
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_PYRAMID_ELEVATOR, 20000.0);
    });
}

/// `bhv_ssl_moving_pyramid_wall_init`.
pub fn init_pyramid_wall(o: &mut SimObject) {
    o.visible = true;
    match bparam2(o) {
        1 => {
            o.pos[1] -= 256.0;
            o.timer += 50.0;
        }
        2 => {
            o.pos[1] -= 512.0;
            o.action = 1;
        }
        _ => {}
    }
}

/// `bhv_ssl_moving_pyramid_wall_loop`.
pub fn pyramid_wall(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        let o = &mut m.objects[id];
        match o.action {
            0 => {
                o.vel[1] = -5.12;
                if o.timer == 100.0 {
                    o.action = 1;
                }
            }
            1 => {
                o.vel[1] = 5.12;
                if o.timer == 100.0 {
                    o.action = 0;
                }
            }
            _ => {}
        }
        o.pos[1] += o.vel[1];
        load_object_collision_model(m, id, K_PYRAMID_WALL, 1000.0);
    });
    // OBJ_FLAG_SET_FACE_ANGLE_TO_MOVE_ANGLE: nothing rotates, the angles stay 0.
}

// ---------------------------------------------------------------------------------------------
// Tox Box (tox_box.inc.c)
// ---------------------------------------------------------------------------------------------

const K_TOX_BOX: &str = "g2/ssl_seg7_collision_tox_box";

/// `TOX_BOX_ACT_*`.
const TOX_INIT: i32 = 0;
const TOX_ROLL_LAND: i32 = 1;
const TOX_IDLE: i32 = 2;
const TOX_FORWARD: i32 = 4;
const TOX_BACKWARD: i32 = 5;
const TOX_RIGHT: i32 = 6;
const TOX_LEFT: i32 = 7;

/// `sToxBoxActionTable1..3` (F/B/R/L = roll + land, `.` = idle), see the movement diagram in
/// tox_box.inc.c.
const TOX_TABLES: [&str; 3] = ["FFRRBBRRB.FFF.BBLLFFLLBBB.F", "FFLLL.RRRBBRBB.FFL", "FFFFF.BBBBBL.RRB.FL"];

fn tox_table(index: usize) -> Vec<i32> {
    let mut out = vec![];
    for c in TOX_TABLES[index.min(2)].chars() {
        match c {
            'F' => out.extend([TOX_FORWARD, TOX_ROLL_LAND]),
            'B' => out.extend([TOX_BACKWARD, TOX_ROLL_LAND]),
            'R' => out.extend([TOX_RIGHT, TOX_ROLL_LAND]),
            'L' => out.extend([TOX_LEFT, TOX_ROLL_LAND]),
            _ => out.push(TOX_IDLE),
        }
    }
    out
}

/// `cur_obj_progress_action_table`.
fn tox_progress(o: &mut SimObject) -> i32 {
    let table = tox_table(o.var("tbt") as usize);
    let next_index = o.var("tbs") as usize + 1;
    if next_index < table.len() {
        o.set_var("tbs", next_index as f64);
        table[next_index]
    } else {
        o.set_var("tbs", 0.0);
        table[0]
    }
}

/// bhvToxBox: `ADD_FLOAT(oPosY, 256)`, `SET_HOME`.
pub fn init_tox_box(o: &mut SimObject) {
    o.visible = true;
    o.pos[1] += 256.0;
    o.home = o.pos;
    o.set_var("tbt", bparam2(o) as f64);
}

/// `cur_obj_set_pos_via_transform` with the object's move angles.
fn set_pos_via_transform(o: &mut SimObject) {
    let t = rotate_zxy_and_translate([0.0; 3], [0, to_s16(o.move_yaw), 0]);
    let (up, left, forward) = (o.var("up") as f32, 0.0f32, o.forward_vel as f32);
    // "Typo, up and left should be swapped": the matrix rows are applied as written.
    o.vel[0] = (t[0][0] * up + t[1][0] * left + t[2][0] * forward) as f64;
    o.vel[1] = (t[0][1] * up + t[1][1] * left + t[2][1] * forward) as f64;
    o.vel[2] = (t[0][2] * up + t[1][2] * left + t[2][2] * forward) as f64;
    for i in 0..3 {
        o.pos[i] += o.vel[i];
    }
}

/// `tox_box_move`.
fn tox_box_move(o: &mut SimObject, forward: f64, up: f64, delta_pitch: f64, mut delta_roll: f64) {
    o.pos[1] = 99.41124 * sins((o.timer + 1.0) / 8.0 * 0x8000 as f64) + o.home[1] + 3.0;
    o.forward_vel = forward;
    o.set_var("up", up);
    let pitch = o.var("pitch") + delta_pitch;
    o.set_var("pitch", pitch);
    if s16(pitch) < 0.0 {
        delta_roll = -delta_roll;
    }
    let roll = o.var("roll") + delta_roll;
    o.set_var("roll", roll);
    set_pos_via_transform(o);
    if o.timer == 7.0 {
        o.action = tox_progress(o);
    }
}

/// `bhv_tox_box_loop` and its action functions.
pub fn tox_box(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        let o = &mut m.objects[id];
        match o.action {
            TOX_INIT => {
                let table = tox_table(o.var("tbt") as usize);
                o.set_var("tbs", 0.0);
                o.action = table[0];
            }
            TOX_ROLL_LAND => {
                o.forward_vel = 0.0;
                o.pos[1] = o.home[1] + 3.0;
                if o.timer == 20.0 {
                    o.action = tox_progress(o);
                }
            }
            TOX_IDLE | 3 => {
                if o.timer == 20.0 {
                    o.action = tox_progress(o);
                }
            }
            TOX_FORWARD => tox_box_move(o, 64.0, 0.0, 0x800 as f64, 0.0),
            TOX_BACKWARD => tox_box_move(o, -64.0, 0.0, -0x800 as f64, 0.0),
            TOX_RIGHT => tox_box_move(o, 0.0, -64.0, 0.0, 0x800 as f64),
            TOX_LEFT => tox_box_move(o, 0.0, 64.0, 0.0, -0x800 as f64),
            _ => {}
        }
        load_object_collision_model(m, id, K_TOX_BOX, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Grindel / Thwomp family (thwomp.inc.c, horizontal_grindel.inc.c)
// ---------------------------------------------------------------------------------------------

const K_GRINDEL: &str = "g2/ssl_seg7_collision_grindel";

pub fn init_grindel(o: &mut SimObject) {
    o.visible = true;
}

/// bhvGrindel: `DROP_TO_FLOOR`, `ADD_FLOAT(oPosY, 1)`, `SET_HOME`, then `bhv_grindel_thwomp_loop`.
pub fn grindel(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        if first_frame(&mut m.objects[id]) {
            let world = m.world.clone();
            let o = &mut m.objects[id];
            drop_to_floor(o, &world);
            o.pos[1] += 1.0;
            o.home = o.pos;
        }
        let o = &mut m.objects[id];
        match o.action {
            // grindel_thwomp_act_raise
            0 => {
                if o.timer > bparam2(o) as f64 + 40.0 {
                    o.action = 1;
                    o.pos[1] += 5.0;
                } else {
                    o.pos[1] += 10.0;
                }
            }
            // grindel_thwomp_act_idle_at_top
            1 => {
                if o.timer == 0.0 {
                    o.set_var("rt", (random_float() * 30.0 + 10.0).trunc());
                }
                if o.timer > o.var("rt") {
                    o.action = 2;
                }
            }
            // grindel_thwomp_act_lower
            2 => {
                o.vel[1] += -4.0;
                o.pos[1] += o.vel[1];
                if o.pos[1] < o.home[1] {
                    o.pos[1] = o.home[1];
                    o.vel[1] = 0.0;
                    o.action = 3;
                }
            }
            // grindel_thwomp_act_land
            3 => {
                if o.timer >= 10.0 {
                    o.action = 4;
                }
            }
            // grindel_thwomp_act_idle_at_bottom
            4 => {
                if o.timer == 0.0 {
                    o.set_var("rt", (random_float() * 10.0 + 20.0).trunc());
                }
                if o.timer > o.var("rt") {
                    o.action = 0;
                }
            }
            _ => {}
        }
        load_object_collision_model(m, id, K_GRINDEL, 1000.0);
    });
}

/// bhvHorizontalGrindel: `SET_OBJ_PHYSICS(40, -400, 0, 1000, 1000, 200)`, `bhv_horizontal_grindel_init`.
pub fn init_horizontal_grindel(o: &mut SimObject) {
    o.visible = true;
    phys::set_physics(o, 40.0, -400.0, 0.0, 1000.0, 1000.0, 200.0);
    o.set_var("target", o.move_yaw);
}

/// `bhv_horizontal_grindel_update`.
pub fn horizontal_grindel(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        // The world handle is released before the collision model is reloaded (`Arc::make_mut`).
        let world = m.world.clone();
        let o = &mut m.objects[id];
        if first_frame(o) {
            drop_to_floor(o, &world);
            o.home = o.pos;
        }
        phys::update_floor_and_walls(o, &world);
        if phys::flags(o) & phys::MASK_ON_GROUND != 0 {
            if o.var("onGround") == 0.0 {
                o.set_var("onGround", 1.0);
                let d = lateral_dist_to_home(o);
                o.set_var("distHome", d);
                o.forward_vel = 0.0;
                o.timer = 0.0;
            }
            let target = o.var("target");
            if rotate_yaw_toward(o, target, 0x400 as f64) {
                if o.timer > 60.0 {
                    if o.var("distHome") > 300.0 {
                        o.set_var("target", s16(target + 0x8000 as f64));
                        o.set_var("distHome", 0.0);
                    } else {
                        o.forward_vel = 11.0;
                        o.vel[1] = 70.0;
                        o.set_var("grav", -4.0);
                        phys::set_flags(o, 0);
                    }
                }
            } else {
                o.timer = 0.0;
            }
        } else {
            o.set_var("onGround", 0.0);
            if o.vel[1] < 0.0 {
                o.set_var("grav", -16.0);
            }
        }
        o.face_yaw = s16(o.move_yaw + 0x4000 as f64);
        phys::move_standard(o, &world, 78.0);
        drop(world);
        load_object_collision_model(m, id, K_GRINDEL, 1000.0);
    });
}

// ---------------------------------------------------------------------------------------------
// Spindel (spindel.inc.c)
// ---------------------------------------------------------------------------------------------

const K_SPINDEL: &str = "g2/ssl_seg7_collision_spindel";

/// `bhv_spindel_init`.
pub fn init_spindel(o: &mut SimObject) {
    o.visible = true;
    o.home[1] = o.pos[1];
    o.set_var("f4", 0.0);
    o.set_var("f8", 0.0);
}

/// `bhv_spindel_loop` (`OBJ_FLAG_SET_FACE_ANGLE_TO_MOVE_ANGLE`: the face pitch follows the move pitch).
pub fn spindel(id: ObjId, m: &mut MarioRuntime) {
    run_platform(m, id, 0, |m, id, _| {
        spindel_script(&mut m.objects[id]);
        let o = &mut m.objects[id];
        let pitch = o.var("mpitch");
        o.set_var("pitch", pitch);
        load_object_collision_model(m, id, K_SPINDEL, 1000.0);
    });
}

fn spindel_script(o: &mut SimObject) {
    if o.var("f4") == -1.0 {
        if o.timer == 32.0 {
            o.set_var("f4", 0.0);
            o.timer = 0.0;
        } else {
            o.vel[2] = 0.0;
            o.set_var("avPitch", 0.0);
            return;
        }
    }
    let mut sp18 = (10.0 - o.var("f4")).abs() as i32;
    sp18 -= 6;
    if sp18 < 0 {
        sp18 = 0;
    }
    if o.timer == (sp18 + 8) as f64 {
        o.timer = 0.0;
        let f4 = o.var("f4") + 1.0;
        o.set_var("f4", f4);
        if f4 == 20.0 {
            let f8 = if o.var("f8") == 0.0 { 1.0 } else { 0.0 };
            o.set_var("f8", f8);
            o.set_var("f4", -1.0);
        }
    }
    sp18 = match sp18 {
        4 | 3 => 4,
        2 | 1 => 2,
        0 => 1,
        other => other,
    };
    if o.timer < (sp18 * 8) as f64 {
        if o.var("f8") == 0.0 {
            o.vel[2] = (20 / sp18) as f64;
            o.set_var("avPitch", (1024 / sp18) as f64);
        } else {
            o.vel[2] = (-20 / sp18) as f64;
            o.set_var("avPitch", (-1024 / sp18) as f64);
        }
        o.pos[2] += o.vel[2];
        let mpitch = o.var("mpitch") + o.var("avPitch");
        o.set_var("mpitch", mpitch);
        let mut lift = sins(mpitch * 4.0) * 23.0;
        if lift < 0.0 {
            lift = -lift;
        }
        o.pos[1] = o.home[1] + lift;
    }
}
