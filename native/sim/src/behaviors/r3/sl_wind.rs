//! Snowman's Land strong wind: the Snowman's blowing (`bhvSLSnowmanWind`, `sl_snowman_wind.inc.c`), the wind particles that
//! push Mario (`bhvStrongWindParticle`, `strong_wind_particle.inc.c`, `INTERACT_STRONG_WIND` -> `ACT_GETTING_BLOWN`) and
//! the walking penguin that blocks it (`bhvSLWalkingPenguin`, `sl_walking_penguin.inc.c`).
//!
//! The wind starts when Mario stands within 1000 of the middle of the ice bridge (1100, 3328, 1164) and the Snowman talks
//! (dialog 153); then within 1500 and 500 of its height it aims at Mario (within 0x1500 of its original yaw) and spawns
//! one particle a frame. A particle within 300 of the penguin's wind-collision point vanishes: Mario hides behind it.
//!
//! Left out: the tiny visual particles (only the collision ones), the cap blow-off, sounds.
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g2::dyn_collision::{load_object_collision_model, run_platform};
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::behaviors::r2a::helpers::{hitbox, update_dialog};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

pub const PENGUIN_MODEL: &str = "actors/penguin/penguin_geo.glb";
const PENGUIN_KEY: &str = "g2/penguin_seg5_collision_05008B88";
/// `penguin_seg5_anims_05008B74`: 0 = walk (frames 5..20), 3 = idle (90).
pub const ANIMS: [AnimSpec; 4] = [spec(5, 0, 0x14, false), spec(0, 0, 0x19, true), spec(0, 0, 0x19, true), spec(0, 0, 0x5A, false)];
pub const ANIM_NAMES: [&str; 4] = ["penguin_seg5_anim_05008B5C", "penguin_seg5_anim_050079E4", "penguin_seg5_anim_05007DCC", "penguin_seg5_anim_050087C0"];

// ---------------------------------------------------------------------------------------------
// Snowman wind
// ---------------------------------------------------------------------------------------------

pub fn init_wind(o: &mut SimObject) {
    o.visible = false;
}

/// `bhv_sl_snowman_wind_loop`.
pub fn wind(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        if m.objects[id].timer == 0.0 {
            let yaw = m.objects[id].move_yaw;
            m.objects[id].set_var("originalYaw", yaw);
        }
        match m.objects[id].action_state as i32 {
            0 => {
                // The textbox check is made from the middle of the bridge.
                let mut probe = m.objects[id].clone();
                probe.pos = [1100.0, 3328.0, 1164.0];
                probe.distance_to_mario = dist_3d(&probe.pos, &m.pos);
                if super::can_mario_activate_textbox(m, &probe, 1000.0, 30.0) {
                    m.objects[id].action_state += 1.0;
                }
            }
            1 => {
                if update_dialog(m, id, 153) {
                    m.objects[id].action_state += 1.0;
                }
            }
            _ => {
                let mario_y = m.pos[1];
                let o = &mut m.objects[id];
                if o.distance_to_mario < 1500.0 && (mario_y - o.home[1]).abs() < 500.0 {
                    let original = o.var("originalYaw");
                    let off = to_s16(o.angle_to_mario - original) as f64;
                    o.move_yaw = if off > 0.0 {
                        if off < 0x1500 as f64 { o.angle_to_mario } else { original + 0x1500 as f64 }
                    } else if off > -0x1500 as f64 {
                        o.angle_to_mario
                    } else {
                        original - 0x1500 as f64
                    };
                    spawn_particle(m, id, 12.0);
                }
            }
        }
    });
}

/// `sStrongWindParticleHitbox`.
const PARTICLE_HITBOX: ObjectHitbox = hitbox(INTERACT_STRONG_WIND, 0.0, 0.0, 0.0, 0.0, 20.0, 70.0, 20.0, 70.0);

fn spawn_particle(m: &mut MarioRuntime, wind: ObjId, spread: f64) {
    let p = spawn_child(m, wind, K::StrongWindParticle, None, spread);
    let (pos, yaw) = (m.objects[wind].pos, m.objects[wind].move_yaw);
    let o = &mut m.objects[p];
    o.pos = pos;
    o.move_yaw = yaw;
    o.set_var("spread", spread);
}

pub fn init_particle(o: &mut SimObject) {
    o.visible = false;
    o.hitbox = PARTICLE_HITBOX;
    o.set_var("realInteract", INTERACT_STRONG_WIND as f64);
}

/// `bhv_strong_wind_particle_loop`.
pub fn particle(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let penguin = m.objects.ids().find(|&i| m.objects[i].behavior == K::SlWalkingPenguin && m.objects[i].active);
        let wind_pt = penguin.map(|p| (m.objects[p].var("windX"), m.objects[p].var("windZ")));
        let o = &mut m.objects[id];
        if o.timer == 0.0 {
            // obj_translate_xyz_random(o, 100) and the spread of the move yaw.
            for k in 0..3 {
                o.pos[k] += (random_float() - 0.5) * 200.0;
            }
            o.forward_vel = 100.0;
            o.vel[1] = 0.0;
            let spread = o.var("spread") * 500.0;
            o.move_yaw += (random_float() - 0.5) * 2.0 * spread;
        }
        cur_obj_move_using_fvel_and_gravity(o, 0.0);
        if o.timer > 15.0 {
            mark_for_deletion(o);
        }
        if let Some((wx, wz)) = wind_pt {
            if js_hypot(wx - o.pos[0], wz - o.pos[2]) < 300.0 {
                mark_for_deletion(o);
                o.hitbox.interact_type = 0;
            }
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Walking penguin
// ---------------------------------------------------------------------------------------------

/// `sSLWalkingPenguinErraticSteps`: (frames, animation, speed, animation speed).
const STEPS: [(i32, usize, f64, f64); 6] = [(60, 0, 6.0, 1.0), (30, 3, 0.0, 1.0), (30, 0, 12.0, 2.0), (30, 3, 0.0, 1.0), (30, 0, -6.0, 1.0), (30, 3, 0.0, 1.0)];

pub fn init_penguin(o: &mut SimObject) {
    o.visible = true;
    o.scale = 6.0;
    phys::set_physics(o, 0.0, -400.0, -50.0, 0.0, 0.0, 200.0);
    anim::init(o, 3, ANIMS[3]);
}

fn anim_accel(o: &mut SimObject, i: usize, accel: f64) {
    anim::init(o, i, ANIMS[i]);
    o.set_var("animAccel", accel);
}

fn turn(o: &mut SimObject) -> bool {
    o.forward_vel = 0.0;
    anim_accel(o, 0, 1.0);
    o.move_yaw += 0x400 as f64;
    o.timer == 31.0
}

/// `bhv_sl_walking_penguin_loop`.
pub fn penguin(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    run_platform(m, id, F_FACE_YAW, |m, id, _| {
        {
            let o = &mut m.objects[id];
            phys::update_floor_and_walls(o, &world);
            match o.action {
                0 => {
                    if o.timer == 0.0 {
                        o.set_var("step", 0.0);
                        o.set_var("stepTimer", 0.0);
                    }
                    let (step, t) = (o.var("step") as usize, o.var("stepTimer"));
                    if t < STEPS[step].0 as f64 {
                        o.set_var("stepTimer", t + 1.0);
                    } else {
                        o.set_var("stepTimer", 0.0);
                        o.set_var("step", if step + 1 >= STEPS.len() { 0.0 } else { step as f64 + 1.0 });
                    }
                    if o.pos[0] < 300.0 {
                        o.action = 1;
                    } else {
                        let s = STEPS[o.var("step") as usize];
                        o.forward_vel = s.2;
                        anim_accel(o, s.1, s.3);
                    }
                }
                1 => {
                    if turn(o) {
                        o.action = 2;
                    }
                }
                2 => {
                    o.forward_vel = 12.0;
                    anim_accel(o, 0, 2.0);
                    if o.pos[0] > 1700.0 {
                        o.action = 3;
                    }
                }
                _ => {
                    if turn(o) {
                        o.action = 0;
                    }
                }
            }
            phys::move_standard(o, &world, -78.0);
            // The point that stops the wind: 60 behind the penguin and 100 perpendicular, away from the Snowman.
            let wx = o.pos[0] + sins(0xDBB0 as f64) * 60.0 + 100.0 * sins(0x1BB0 as f64);
            let wz = o.pos[2] + coss(0xDBB0 as f64) * 60.0 + 100.0 * coss(0x1BB0 as f64);
            o.set_var("windX", wx);
            o.set_var("windZ", wz);
            anim::advance(o);
        }
        load_object_collision_model(m, id, PENGUIN_KEY, 1000.0);
    });
}
