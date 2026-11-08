//! Objects of the swimming set: the underwater Koopa shell (`bhvKoopaShellUnderwater`,
//! `src/game/behaviors/koopa_shell_underwater.inc.c`) and the DDD whirlpool (`bhvWhirlpool`,
//! `src/game/behaviors/whirlpool.inc.c`). Mario's side lives in `actions/water.rs`.
use crate::actions::water::interact_whirlpool;
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

/// `sKoopaShellUnderwaterHitbox`.
const SHELL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_GRABBABLE,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 1.0,
    num_loot_coins: 0.0,
    radius: 80.0,
    height: 50.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

/// `sWhirlpoolHitbox`.
const WHIRLPOOL_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_WHIRLPOOL,
    down_offset: 0.0,
    damage_or_coin_value: 0.0,
    health: 0.0,
    num_loot_coins: 0.0,
    radius: 200.0,
    height: 500.0,
    hurtbox_radius: 0.0,
    hurtbox_height: 0.0,
};

pub fn init_koopa_shell_underwater(object: &mut SimObject) {
    object.set_hitbox(SHELL_HITBOX);
    object.visible = true;
}

/// decomp: `bhv_koopa_shell_underwater_loop`. Thrown or dropped (or released by the shell
/// swimming timeout, which sets `INT_STATUS_STOP_RIDING`) the shell vanishes in a puff of mist.
pub fn koopa_shell_underwater_behavior(id: ObjId, m: &mut MarioRuntime) {
    let o = &mut m.objects[id];
    match o.held_state {
        HELD_FREE => o.set_hitbox(SHELL_HITBOX),
        HELD_THROWN | HELD_DROPPED => o.deactivate(-1.0),
        _ => {}
    }
    if o.interact_status & INT_STATUS_STOP_RIDING != 0 {
        o.deactivate(-1.0);
    }
    o.interact_status = 0;
}

pub fn init_whirlpool(object: &mut SimObject) {
    object.set_hitbox(WHIRLPOOL_HITBOX);
    object.visible = true;
}

/// decomp: `bhv_whirlpool_loop` plus the `INTERACT_WHIRLPOOL` branch of
/// `mario_process_interactions`. `oInteractStatus` is never cleared by the behavior, so a
/// whirlpool catches Mario once per area load; the object forgets when Mario respawns.
pub fn whirlpool_behavior(id: ObjId, m: &mut MarioRuntime) {
    let respawns = m.respawns;
    {
        let o = &mut m.objects[id];
        o.set_hitbox(WHIRLPOOL_HITBOX);
        o.face_yaw = to_s16(o.face_yaw + 8000.0) as f64;
        o.visible = o.distance_to_mario < 5000.0;
        if o.interact_status & INT_STATUS_INTERACTED != 0 && o.var("caughtRespawns") != respawns {
            o.interact_status = 0;
        }
    }
    let touching = mario_overlaps_object(m, &m.objects[id]);
    if touching && m.action & ACT_FLAG_INTANGIBLE == 0 && m.objects[id].interact_status & INT_STATUS_INTERACTED == 0 {
        if interact_whirlpool(m, id) {
            m.objects[id].set_var("caughtRespawns", respawns);
        }
    }
}
