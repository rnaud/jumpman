//! SM64 round 2, part B: castle NPCs (Toad, Mips tail), coins (blue coin switch, hidden / moving
//! blue coins, recovery hearts), exclamation boxes with coin / 1-up / shell contents, the 1-up
//! variants and the PSS slide timer. Every file cites the decomp function it ports
//! (`reference/sm64/src/game/**`). Conventions are those of `behaviors/g1` (`action` / `action_timer`
//! advanced by [`end`], vars for the `o*` fields).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::object_manager::BehaviorTable;
use ObjectBehaviorKind as K;

pub mod blue_coin;
pub mod excl_box;
pub mod moat;
pub mod npc;
pub mod one_ups;
pub mod pss;
pub mod toad;

pub fn register(t: &mut BehaviorTable) {
    t.register(K::BlueCoinSwitch, blue_coin::init_switch, blue_coin::switch_update);
    t.register(K::HiddenBlueCoin, blue_coin::init_hidden, blue_coin::hidden_update);
    t.register(K::MovingBlueCoin, blue_coin::init_moving, blue_coin::moving_update);
    t.register(K::RecoveryHeart, blue_coin::init_heart, blue_coin::heart_update);
    t.register(K::ExclamationBox, excl_box::init, excl_box::update);
    t.register(K::OneUpWalking, one_ups::init, one_ups::walking);
    t.register(K::OneUpRunning, one_ups::init, one_ups::running_away);
    t.register(K::OneUpSliding, one_ups::init, one_ups::sliding);
    t.register(K::Hidden1Up, one_ups::init_hidden, one_ups::hidden);
    t.register(K::Hidden1UpTrigger, one_ups::init_trigger, one_ups::trigger);
    t.register(K::Hidden1UpInPole, one_ups::init_in_pole, one_ups::in_pole);
    t.register(K::Hidden1UpInPoleTrigger, one_ups::init_in_pole_trigger, one_ups::in_pole_trigger);
    t.register(K::Hidden1UpInPoleSpawner, one_ups::init_in_pole_spawner, one_ups::in_pole_spawner);
    t.register(K::ToadMessage, toad::init, toad::update);
    t.set_interact(K::ToadMessage, toad::interact);
    t.register(K::PssTimer, pss::init, pss::update);
    t.register(K::WaterLevelPillar, moat::init_pillar, moat::pillar);
    t.register(K::MoatGrill, moat::init_grill, moat::grill);
}

/// `obj_flicker_and_disappear` (object_helpers.c): blink for 40 frames after `life_span`, then
/// delete. Returns true once deleted.
pub fn flicker_and_disappear(o: &mut SimObject, life_span: f64) -> bool {
    if o.action_timer < life_span {
        return false;
    }
    if o.action_timer < life_span + 40.0 {
        o.visible = o.action_timer % 2.0 == 0.0;
        false
    } else {
        o.deactivate(-1.0);
        true
    }
}

/// `is_point_within_radius_of_mario`.
pub fn near_mario(m: &MarioRuntime, p: &Vec3, dist: f64) -> bool {
    let (dx, dy, dz) = (p[0] - m.pos[0], p[1] - m.pos[1], p[2] - m.pos[2]);
    dx * dx + dy * dy + dz * dz < dist * dist
}

/// `cur_obj_nearest_object_with_behavior`: the nearest active object of a kind (None if none).
pub fn nearest_of(m: &MarioRuntime, from: ObjId, kind: ObjectBehaviorKind) -> Option<ObjId> {
    let p = m.objects[from].pos;
    let mut best: Option<(f64, ObjId)> = None;
    for (i, o) in m.objects.objects.iter().enumerate() {
        if o.behavior != kind || !o.active || i == from.idx() {
            continue;
        }
        let d = dist_3d(&o.pos, &p);
        if best.map_or(true, |(bd, _)| d < bd) {
            best = Some((d, ObjId(i as u32)));
        }
    }
    best.map(|(_, id)| id)
}

/// End of an object update (`cur_obj_update`): `oTimer++` and the action-change reset.
pub fn end(o: &mut SimObject) {
    o.action_timer += 1.0;
    if o.action != o.prev_action {
        o.action_timer = 0.0;
        o.action_state = 0.0;
        o.prev_action = o.action;
    }
}
