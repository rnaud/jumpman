//! Dire, Dire Docks: the moving poles (ddd_pole.inc.c) and the Sushi sharks (sushi.inc.c).
use super::obj::*;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

/// bhvDDDPole: `SET_HITBOX(80, 800)`, `INTERACT_POLE`, `hitboxDownOffset = 100`,
/// `oDDDPoleMaxOffset = 100 * oBhvParams2ndByte`, `oDDDPoleVel = 10`.
/// (The decomp deletes the poles until the upstairs key is owned; the sim keeps them.)
pub fn init_ddd_pole(o: &mut SimObject) {
    o.visible = true;
    o.hitbox = ObjectHitbox { interact_type: INTERACT_POLE, down_offset: 100.0, damage_or_coin_value: 0.0, health: 0.0, num_loot_coins: 0.0, radius: 80.0, height: 800.0, hurtbox_radius: 80.0, hurtbox_height: 800.0 };
    let tenth = ((o.param as u32) >> 16 & 0xFF) as f64;
    o.set_var("max", 100.0 * tenth);
    o.set_var("vel", 10.0);
    o.set_var("offset", 0.0);
}

/// `bhv_ddd_pole_update` + `bhv_pole_base_loop`.
pub fn ddd_pole(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, 0, |m, id| {
        let o = &mut m.objects[id];
        if o.timer > 20.0 {
            let mut offset = o.var("offset") + o.var("vel");
            let max = o.var("max");
            let mut clamped = false;
            if offset < 0.0 {
                offset = 0.0;
                clamped = true;
            } else if offset > max {
                offset = max;
                clamped = true;
            }
            o.set_var("offset", offset);
            if clamped {
                let v = -o.var("vel");
                o.set_var("vel", v);
                o.timer = 0.0;
            }
        }
        // obj_set_dist_from_home
        let offset = o.var("offset");
        o.pos[0] = o.home[0] + offset * coss(o.move_yaw);
        o.pos[2] = o.home[2] + offset * sins(o.move_yaw);
        // bhv_pole_base_loop
        let (y, h, timer) = (o.pos[1], o.hitbox.height, o.timer);
        if y - 10.0 < m.pos[1] && m.pos[1] < y + h + 30.0 && timer > 10.0 && (m.action & crate::state::MARIO_PUNCHING) == 0 {
            push_mario_away(m, id, 70.0);
        }
    });
}

/// bhvSushiShark: `SET_HITBOX_WITH_OFFSET(100, 50, 50)`, `INTERACT_DAMAGE`, damage 3.
pub fn init_sushi_shark(o: &mut SimObject) {
    o.visible = true;
    o.hitbox = ObjectHitbox { interact_type: INTERACT_DAMAGE, down_offset: 50.0, damage_or_coin_value: 3.0, health: 0.0, num_loot_coins: 0.0, radius: 100.0, height: 50.0, hurtbox_radius: 100.0, hurtbox_height: 50.0 };
}

/// `bhv_sushi_shark_loop`: circles the pool (radius 1700) around its home, bobbing 200 units about
/// `water level + oHomeY`.
pub fn sushi_shark(id: ObjId, m: &mut MarioRuntime) {
    run(m, id, F_FACE_YAW, |m, id| {
        let water = {
            let o = &m.objects[id];
            m.world.water_level(o.pos[0], o.pos[2])
        };
        let o = &mut m.objects[id];
        let f4 = o.var("f4");
        o.pos[0] = o.home[0] + sins(f4) * 1700.0;
        o.pos[2] = o.home[2] + coss(f4) * 1700.0;
        o.pos[1] = water + o.home[1] + sins(f4) * 200.0;
        o.move_yaw = to_s16(f4 + 0x4000 as f64) as f64;
        o.set_var("f4", to_s16(f4 + 0x80 as f64) as f64);
        o.interact_status = 0;
    });
}
