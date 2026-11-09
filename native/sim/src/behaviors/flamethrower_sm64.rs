//! Port of `src/game/behaviors/flamethrower.inc.c` (`bhvFlamethrower` / `bhvFlamethrowerFlame`),
//! the fire jets of Rainbow Ride, Bowser in the Dark World, Fire Sea and Sky.
//!
//! The jet wakes up when Mario is within 2000 units, spews flames for 74 frames (15-frame flame
//! lifetime for 60 frames, then shrinking to 2), rests for 60 frames and repeats. The flames are
//! `sm64FlamethrowerFlame` objects; their `INTERACT_FLAME` hitbox sets Mario on fire
//! (`g2::interact::interact_flame`, registered in `behaviors/mod.rs`).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::solids::random_float;

pub fn init_flamethrower(o: &mut SimObject) {
    o.visible = false;
    o.set_var("oTimer", 0.0);
}

/// `bhv_flamethrower_loop`. `param` = `oBhvParams2ndByte`.
pub fn flamethrower_behavior(id: ObjId, m: &mut MarioRuntime) {
    let dist = m.objects[id].distance_to_mario;
    let (action, timer, param, pos, yaw) = {
        let o = &mut m.objects[id];
        (o.action, o.var("oTimer"), o.param as i32, o.pos, o.move_yaw)
    };
    let mut next = action;
    let mut spawn = false;
    match action {
        0 => {
            if dist < 2000.0 {
                next = 1;
            }
        }
        1 => {
            let mut lifetime = 1.0;
            if timer < 60.0 {
                lifetime = 15.0;
            } else if timer < 74.0 {
                lifetime = 75.0 - timer;
            } else {
                next = 2;
            }
            m.objects[id].set_var("flameLife", lifetime);
            spawn = true;
        }
        _ => {
            if timer > 60.0 {
                next = 0;
            }
        }
    }
    if spawn {
        let mut spec = ObjectSpec::new(ObjectBehaviorKind::SmFlamethrowerFlame, pos, yaw);
        spec.param = Some(param as f64);
        spec.model = Some(if param == 1 { "actors/flame/blue_flame_geo.glb" } else { "actors/flame/red_flame_geo.glb" }.to_string());
        let nid = m.objects.spawn(&spec);
        let f = &mut m.objects[nid];
        f.forward_vel = if param == 2 { 50.0 } else { 95.0 };
        f.set_var("parent", id.0 as f64);
        f.set_var("bp2", param as f64);
    }
    let o = &mut m.objects[id];
    let t = if next != action { 0.0 } else { timer + 1.0 };
    o.action = next;
    o.set_var("oTimer", t);
}

/// `SET_HITBOX_WITH_OFFSET(50, 25, 25)`.
pub fn init_flame(o: &mut SimObject) {
    o.billboard = true;
    o.visible = true;
    o.set_var("oTimer", 0.0);
    o.set_hitbox(ObjectHitbox { interact_type: crate::behaviors::g2::interact::INTERACT_FLAME, down_offset: 25.0, damage_or_coin_value: 1.0, health: 0.0, num_loot_coins: 0.0, radius: 50.0, height: 25.0, hurtbox_radius: 0.0, hurtbox_height: 0.0 });
}

/// `bhv_flamethrower_flame_loop`.
pub fn flame_behavior(id: ObjId, m: &mut MarioRuntime) {
    let parent = ObjId(m.objects[id].var("parent") as u32);
    let life = m.objects[parent].var("flameLife");
    let o = &mut m.objects[id];
    let t = o.var("oTimer");
    let bp2 = o.var("bp2") as i32;
    if t == 0.0 {
        o.pos[0] += random_float() * 10.0 - 5.0;
        o.pos[1] += random_float() * 10.0 - 5.0;
        o.pos[2] += random_float() * 10.0 - 5.0;
    }
    let scale = if bp2 == 2 { t * (o.forward_vel - 6.0) / 100.0 + 2.0 } else { t * (o.forward_vel - 20.0) / 100.0 + 1.0 };
    let life = if bp2 == 3 { (life / 1.2).trunc() } else { life };
    o.scale = scale;
    if bp2 == 4 {
        o.pos[1] += o.forward_vel;
    } else {
        o.pos[0] += o.forward_vel * sins(o.move_yaw);
        o.pos[2] += o.forward_vel * coss(o.move_yaw);
    }
    if t > life {
        o.deactivate(-1.0);
        return;
    }
    o.set_var("oTimer", t + 1.0);
}
