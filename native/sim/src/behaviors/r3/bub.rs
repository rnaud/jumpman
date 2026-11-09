//! Chirp Chirp (`bhvChirpChirp` -> `bhv_bub_spawner_loop`) and its Bub, the cheep-cheep that swims near the water surface of
//! Dire, Dire Docks (`bhvBub`). Port of `src/game/behaviors/bub.inc.c`.
//!
//! The spawner (invisible) releases one Bub when Mario is within 1500 and takes it back when Mario is 2000 above it. The Bub
//! keeps just under the surface, follows Mario slowly, backs off after brushing past, and stings for 1 (INTERACT_DAMAGE, 20 x 10).
use crate::behaviors::g1::anim::{self, spec, AnimSpec};
use crate::behaviors::g1::hitbox;
use crate::behaviors::g2::obj::*;
use crate::behaviors::g2::phys;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::ObjectBehaviorKind as K;
use crate::object::*;

pub const BUB_MODEL: &str = "actors/bub/bub_geo.glb";
/// `bub_seg6_anims_06012354`: one 40-frame swim loop.
pub const ANIMS: [AnimSpec; 1] = [spec(0, 0, 0x28, false)];
pub const ANIM_NAMES: [&str; 1] = ["bub_seg6_anim_0601233C"];

pub fn init_spawner(o: &mut SimObject) {
    o.visible = false;
    // `SET_INT(oBirdChirpChirpUnkF4, 1)`: one Bub per Chirp Chirp.
    o.set_var("count", 1.0);
}

/// `bhv_bub_spawner_loop` (`bub_spawner_act_0..3`).
pub fn spawner(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| match m.objects[id].action {
        0 => {
            if m.objects[id].distance_to_mario < 1500.0 {
                for _ in 0..m.objects[id].var("count") as usize {
                    spawn_child(m, id, K::Bub, Some(BUB_MODEL), 0.0);
                }
                m.objects[id].action = 1;
            }
        }
        1 => {
            if m.pos[1] - m.objects[id].pos[1] > 2000.0 {
                m.objects[id].action = 2;
            }
        }
        2 => m.objects[id].action = 3,
        _ => m.objects[id].action = 0,
    });
}

pub fn init_bub(o: &mut SimObject) {
    o.visible = true;
    o.hitbox = hitbox(INTERACT_DAMAGE, 10.0, 1.0, 0.0, 0.0, 20.0, 10.0, 20.0, 10.0);
    o.set_var("realInteract", INTERACT_DAMAGE as f64);
    phys::set_physics(o, 30.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    anim::init(o, 0, ANIMS[0]);
}

/// `bub_move_vertically(speed)`: only inside the band around the parent's height.
fn move_vertically(o: &mut SimObject, parent_y: f64, speed: f64) {
    let extra = o.var("band");
    if parent_y - 100.0 - extra < o.pos[1] && o.pos[1] < parent_y + 1000.0 + extra {
        let goal = o.var("goalY");
        approach_f32_signed_to(&mut o.pos[1], goal, speed);
    }
}

fn approach_f32_signed_to(value: &mut f64, target: f64, inc: f64) {
    if *value < target {
        *value = (*value + inc).min(target);
    } else {
        *value = (*value - inc).max(target);
    }
}

/// `bhv_bub_loop`.
pub fn bub(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let world = m.world.clone();
        let (mario, parent) = (m.pos, m.objects[id].link);
        let parent_y = parent.map_or(m.objects[id].home[1], |p| m.objects[p].pos[1]);
        let parent_resets = parent.is_some_and(|p| m.objects[p].action == 2);
        let o = &mut m.objects[id];
        let water = world.water_level(o.pos[0], o.pos[2]);
        o.set_var("surface", water);
        phys::update_floor_and_walls(o, &world);
        match o.action {
            0 => {
                o.set_var("rise", random_float() * 100.0);
                o.set_var("band", random_float() * 300.0);
                o.action = 1;
            }
            1 => act1(o, mario, parent_y),
            _ => act2(o, mario, parent_y),
        }
        o.set_var("goalY", mario[1] + o.var("rise"));
        cur_obj_move_using_fvel_and_gravity(o, 0.0);
        if parent_resets {
            mark_for_deletion(o);
            return;
        }
        anim::advance(o);
    });
}

fn angle_to_home(o: &SimObject) -> f64 {
    atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64
}

fn mario_lateral_dist_to_home(o: &SimObject, mario: Vec3) -> f64 {
    js_hypot(mario[0] - o.home[0], mario[2] - o.home[2])
}

fn follow_surface(o: &mut SimObject, mario: Vec3, parent_y: f64, near: f64) {
    let mut dy = o.pos[1] - mario[1];
    let surface = o.var("surface");
    if o.pos[1] < surface - 50.0 {
        dy = dy.abs();
        move_vertically(o, parent_y, if dy < 500.0 { near } else { 4.0 });
    } else {
        o.pos[1] = surface - 50.0;
        if dy > 300.0 {
            o.pos[1] -= 1.0;
        }
    }
}

fn act1(o: &mut SimObject, mario: Vec3, parent_y: f64) {
    if o.timer == 0.0 {
        o.forward_vel = random_float() * 2.0 + 2.0;
        o.set_var("sting", random_float());
    }
    follow_surface(o, mario, parent_y, 1.0);
    if mario_lateral_dist_to_home(o, mario) > 800.0 {
        o.angle_to_mario = angle_to_home(o);
    }
    let target = o.angle_to_mario;
    rotate_yaw_toward(o, target, 0x100 as f64);
    if (o.distance_to_mario < 200.0 && o.var("sting") < 0.5) || o.interact_status & INT_STATUS_INTERACTED != 0 {
        o.action = 2;
    }
}

fn act2(o: &mut SimObject, mario: Vec3, parent_y: f64) {
    if o.timer >= 20.0 {
        o.interact_status = 0;
    }
    if o.forward_vel == 0.0 {
        o.forward_vel = 6.0;
    }
    follow_surface(o, mario, parent_y, 2.0);
    if mario_lateral_dist_to_home(o, mario) > 800.0 {
        o.angle_to_mario = angle_to_home(o);
    }
    let target = to_s16(o.angle_to_mario + 0x8000 as f64) as f64;
    rotate_yaw_toward(o, target, 0x400 as f64);
    if o.timer > 200.0 && o.distance_to_mario > 600.0 {
        o.action = 1;
    }
}

