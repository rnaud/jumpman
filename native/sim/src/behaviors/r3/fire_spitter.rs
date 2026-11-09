//! Fire Spitter (`macro_fire_spitter` -> `bhvFireSpitter`, a floating bowling ball in WDW, THI, VCUTM, BitS ...): port of
//! `src/game/behaviors/fire_spitter.inc.c`.
//!
//! It shrinks from its start scale 0.4 toward 0.2; once 150 frames have passed and Mario is within 800 (and the
//! ball is not in water) it swells, turns toward Mario and spits one flame (`obj_spit_fire(0, 0, 0, 5.0, ... 20, 15, 0x1000)`),
//! then settles back to scale 0.1. `bhvFireSpitter` has no `SET_OBJ_PHYSICS`: no gravity, it floats where it is placed.
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::behaviors::g2::thi::spit_fire;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

pub const MODEL: &str = "actors/bowling_ball/bowling_ball_geo.glb";

pub fn init(o: &mut SimObject) {
    o.visible = true;
    o.scale = 0.4;
    phys::set_physics(o, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
}

/// `approach_f32_ptr` (object_helpers.c): true when the value is at `target`.
fn approach(value: &mut f64, target: f64, inc: f64) -> bool {
    if *value > target {
        *value = (*value - inc).max(target);
    } else {
        *value = (*value + inc).min(target);
    }
    *value == target
}

/// `obj_grow_then_shrink`: 0 while growing, -1 once the scale is back at `end`, 1 on the frame the flame is shot.
fn grow_then_shrink(o: &mut SimObject, shoot_scale: f64, end: f64) -> i32 {
    let mut vel = o.var("scaleVel");
    let mut status = 0;
    if o.timer < 2.0 {
        o.scale += vel;
        vel -= 0.01;
        if vel > -0.03 {
            o.timer = 0.0;
        }
    } else if o.timer > 10.0 {
        if approach(&mut o.scale, end, 0.05) {
            status = -1;
        } else if vel != 0.0 && o.scale < shoot_scale {
            vel = 0.0;
            status = 1;
        }
    }
    o.set_var("scaleVel", vel);
    status
}

/// `bhv_fire_spitter_update`.
pub fn update(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run(m, id, 0, |m, id| {
        let mut shot = false;
        {
            let o = &mut m.objects[id];
            phys::update_floor_and_walls(o, &world);
            match o.action {
                0 => {
                    approach(&mut o.scale, 0.2, 0.002);
                    if o.timer > 150.0 && o.distance_to_mario < 800.0 && phys::flags(o) & phys::MASK_IN_WATER == 0 {
                        o.action = 1;
                        o.set_var("scaleVel", 0.05);
                    }
                }
                _ => {
                    o.move_yaw = o.angle_to_mario;
                    match grow_then_shrink(o, 0.15, 0.1) {
                        0 => {}
                        s if s < 0 => o.action = 0,
                        _ => shot = true,
                    }
                }
            }
            phys::move_standard(o, &world, -78.0);
        }
        if shot {
            spit_fire(m, id, 0.0, 0.0, 5.0, 20.0, 15.0, 0x1000 as f64);
        }
    });
}
