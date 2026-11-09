//! Big Bully boss from Lethal Lava Land: three hits, strong contact damage
//! (src/sim/behaviors/bigBully.ts).
//!
//! The Big Chill Bully of Snowman's Land (`bhvBigChillBully`, `oBullySubtype = BULLY_STYPE_CHILL`) shares the
//! behavior; its two decomp-specific rules are ported from bully.inc.c: `bully_step` sends it to
//! `BULLY_ACT_LAVA_DEATH` once it falls below y = 1030 (knocked off the ice into the freezing water), and
//! `bully_act_level_death` then spawns its star with `spawn_default_star(130, 1600, -4335)` (BPARAM1 =
//! STAR_INDEX_ACT_2) instead of ending the mission like the Big Bully.
use super::koopa::{js_cos_yaw, js_sin_yaw, var_or};
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;

const BIG_BULLY_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_DAMAGE | INTERACT_BREAKABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 3.0,
    num_loot_coins: 0.0,
    radius: 170.0,
    height: 280.0,
    hurtbox_radius: 150.0,
    hurtbox_height: 260.0,
};

pub fn init_big_bully(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.scale = 3.0;
    object.set_hitbox(BIG_BULLY_HITBOX);
    object.set_var("health", 3.0);
    object.set_var("timer", 0.0);
    object.visible = true;
}

/// bhvBigChillBully: `SET_INT(oBullySubtype, BULLY_STYPE_CHILL)` + `bhv_big_bully_init`.
pub fn init_big_chill_bully(object: &mut SimObject) {
    init_big_bully(object);
    object.set_var("chill", 1.0);
}

/// Where the Big Chill Bully's star flies (`bully_act_level_death`, bully.inc.c).
pub const CHILL_BULLY_STAR: Vec3 = [130.0, 1600.0, -4335.0];
/// `bully_step`: below this height the chill bully is in the freezing water (`BULLY_ACT_LAVA_DEATH`).
pub const CHILL_BULLY_DEATH_Y: f64 = 1030.0;

fn defeated(id: ObjId, m: &mut MarioRuntime) {
    let pos = m.objects[id].pos;
    let chill = m.objects[id].var("chill") != 0.0;
    m.objects[id].deactivate(-1.0);
    if chill {
        // spawn_mist_particles + spawn_default_star(130.0f, 1600.0f, -4335.0f)
        m.objects.events.push(crate::object_manager::ObjectEvent::Break { pos, wall: None });
        crate::behaviors::r2a::helpers::spawn_default_star(m, id, CHILL_BULLY_STAR);
    } else {
        m.objects.big_bully_defeated(pos);
    }
}

pub fn big_bully_behavior(id: ObjId, m: &mut MarioRuntime) {
    let world = m.world.clone();
    let mpos = m.pos;
    let object = &mut m.objects[id];
    if object.interact_status & INT_STATUS_WAS_ATTACKED != 0 {
        object.interact_status = 0;
        let health = var_or(object, "health", 3.0) - 1.0;
        if health <= 0.0 {
            defeated(id, m);
            return;
        }
        object.set_var("health", health);
        object.set_var("flash", 12.0);
        object.pos[0] += js_sin_yaw(object.face_yaw) * 50.0;
        object.pos[2] += js_cos_yaw(object.face_yaw) * 50.0;
        return;
    }
    if var_or(object, "flash", 0.0) > 0.0 { let f = object.var("flash") - 1.0; object.set_var("flash", f); }
    let t = var_or(object, "timer", 0.0) + 1.0;
    object.set_var("timer", t);
    if object.distance_to_mario < 2200.0 {
        object.move_yaw = atan2s(mpos[2] - object.pos[2], mpos[0] - object.pos[0]) as f64;
        object.face_yaw = object.move_yaw;
        object.forward_vel = 5.0;
    } else {
        object.forward_vel = 0.0;
    }
    object.step_physics(&world, None);
    if object.var("chill") != 0.0 && object.pos[1] < CHILL_BULLY_DEATH_Y {
        defeated(id, m);
    }
}
