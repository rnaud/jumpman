//! Port of `src/sim/behaviors/kingBobomb.ts` (king_bobomb.inc.c, three-throw core).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::object::*;
use crate::state::*;

const KING_ACT_IDLE: i32 = 0;
const KING_ACT_THROWN: i32 = 1;
const KING_ACT_RETURN_HOME: i32 = 5;

const KING_HITBOX: ObjectHitbox = ObjectHitbox {
    interact_type: INTERACT_GRABBABLE,
    down_offset: 0.0,
    damage_or_coin_value: 1.0,
    health: 3.0,
    num_loot_coins: 0.0,
    radius: 100.0,
    height: 100.0,
    hurtbox_radius: 100.0,
    hurtbox_height: 100.0,
};

/// `readyForNpcDialog` (actions/cutscene.ts).
pub fn ready_for_npc_dialog(m: &MarioRuntime) -> bool {
    let group = m.action & ACT_GROUP_MASK;
    (group == ACT_GROUP_STATIONARY || group == ACT_GROUP_MOVING)
        && m.action & (ACT_FLAG_RIDING_SHELL | ACT_FLAG_INVULNERABLE) == 0
}

pub fn init_king_bobomb(object: &mut SimObject) {
    object.gravity = 4.0;
    object.friction = 1.0;
    object.buoyancy = 2.0;
    object.set_hitbox(ObjectHitbox { interact_type: 0, ..KING_HITBOX });
    object.set_action(KING_ACT_IDLE);
    object.set_var("health", 3.0);
    object.set_var("thrown", 0.0);
    object.set_var("grabbing", 0.0);
    object.set_var("grabCooldown", 0.0);
    object.set_var("intro", 0.0);
    object.set_var("dialog", 0.0);
    object.visible = true;
}

fn throw_landing(object: &mut SimObject) {
    if !object.grounded { return; }
    if object.var("thrown") == 0.0 { return; }
    object.set_var("thrown", 0.0);
    object.set_var("grabbing", 0.0);
    object.set_var("grabCooldown", 0.0);
    if object.pos[1] <= object.home[1] - 100.0 {
        begin_return(object);
        return;
    }
    let h = object.var("health") - 1.0;
    object.set_var("health", h);
    object.forward_vel = 0.0;
    object.vel[1] = 0.0;
    object.face_yaw = object.move_yaw;

    if h <= 0.0 {
        object.set_var("dialog", 116.0);
        object.hitbox.interact_type = 0;
        return;
    }

    object.hitbox.interact_type = INTERACT_GRABBABLE;
    object.set_action(KING_ACT_IDLE);
}

fn begin_return(o: &mut SimObject) {
    o.set_action(KING_ACT_RETURN_HOME);
    o.hitbox.interact_type = 0;
    o.forward_vel = 0.0;
    o.move_yaw = atan2s(o.home[2] - o.pos[2], o.home[0] - o.pos[0]) as f64;
    o.face_yaw = o.move_yaw;
}

fn return_home(o: &mut SimObject) {
    if o.action_state == 0.0 {
        if o.pos[1] < o.home[1] { o.pos[1] += 100.0; return; }
        // arc_to_goal_pos with vertical speed 100 and gravity -4.
        o.vel[1] = 100.0;
        o.forward_vel = js_hypot(o.home[0] - o.pos[0], o.home[2] - o.pos[2]) / 49.0;
        o.action_state = 1.0;
    }
    o.vel[1] -= 4.0;
    o.pos[1] += o.vel[1];
    o.pos[0] += o.forward_vel * sins(o.move_yaw);
    o.pos[2] += o.forward_vel * coss(o.move_yaw);
    if o.vel[1] < 0.0 && o.pos[1] <= o.home[1] {
        o.pos = o.home;
        o.vel[1] = 0.0;
        o.forward_vel = 0.0;
        o.set_action(KING_ACT_IDLE);
        o.set_var("intro", 0.0);
        o.set_var("returnWarning", 1.0);
    }
}

pub fn king_bobomb_behavior(id: ObjId, m: &mut MarioRuntime) {
    if m.objects[id].var("homeSet") == 0.0 {
        let home = m.objects[id].home;
        let floor = m.world.find_floor(home[0], home[1], home[2]);
        let object = &mut m.objects[id];
        if floor.surface.is_some() { object.home[1] = floor.height; }
        object.set_var("homeSet", 1.0);
    }
    if m.objects[id].var("dialog") != 0.0 {
        if m.objects[id].var("dialogDone") != 0.0 {
            if m.objects[id].var("dialog") == 116.0 {
                let pos = m.objects[id].pos;
                m.objects.boss_defeated(pos);
                m.objects[id].deactivate(-1.0);
            } else {
                let object = &mut m.objects[id];
                object.set_var("intro", 2.0);
                object.hitbox.interact_type = INTERACT_GRABBABLE;
            }
            let object = &mut m.objects[id];
            object.set_var("dialog", 0.0);
            object.set_var("dialogDone", 0.0);
            object.set_var("dialogStarted", 0.0);
        } else if m.objects[id].var("dialogStarted") == 0.0 && ready_for_npc_dialog(m) {
            let d = m.objects[id].var("dialog");
            m.objects.king_dialog(id, d);
            m.objects[id].set_var("dialogStarted", 1.0);
        }
        return;
    }
    if m.objects[id].action == KING_ACT_RETURN_HOME { return_home(&mut m.objects[id]); return; }
    match m.objects[id].held_state {
        HELD_HELD => {
            m.objects[id].visible = true;
            return;
        }
        HELD_DROPPED | HELD_THROWN => {
            let object = &mut m.objects[id];
            object.held_state = HELD_FREE;
            object.set_action(KING_ACT_THROWN);
            object.set_var("thrown", 1.0);
            object.forward_vel = 20.0;
            object.vel[1] = 50.0;
            object.pos[1] += 20.0;
            object.hitbox.interact_type = 0;
            object.gravity = 4.0;
        }
        _ => {}
    }

    if m.objects[id].var("thrown") != 0.0 {
        let object = &mut m.objects[id];
        object.step_physics(&m.world, None);
        throw_landing(object);
        return;
    }

    if m.objects[id].var("grabbing") != 0.0 {
        let object = &mut m.objects[id];
        let gt = object.var("grabTimer") - 1.0;
        object.set_var("grabTimer", gt);
        object.forward_vel = 0.0;
        if gt <= 0.0 {
            object.set_var("grabbing", 0.0);
            object.set_var("grabCooldown", 150.0);
            object.set_var("thrown", 0.0);
            m.throw_from_grab(id);
        }
        let object = &mut m.objects[id];
        object.step_physics(&m.world, None);
        object.face_yaw = object.move_yaw;
        return;
    }

    let mpos = m.pos;
    if m.objects[id].pos[1] - mpos[1] > 1200.0 {
        let object = &m.objects[id];
        if object.var("intro") != 0.0 || object.var("returnWarning") != 0.0 { m.objects.king_leave(); }
        let object = &mut m.objects[id];
        object.set_var("health", 3.0);
        object.set_var("intro", 0.0);
        object.set_var("returnWarning", 0.0);
    }
    if m.objects[id].var("intro") == 0.0 {
        let (dy, dist) = {
            let object = &mut m.objects[id];
            object.pos = object.home;
            object.forward_vel = 0.0;
            object.vel[1] = 0.0;
            object.hitbox.interact_type = 0;
            (mpos[1] - object.pos[1], js_hypot(mpos[0] - object.pos[0], mpos[2] - object.pos[2]))
        };
        if dy > -160.0 && dy < 100.0 && ready_for_npc_dialog(m) && m.action & ACT_FLAG_AIR == 0 && dist < 500.0 {
            let object = &mut m.objects[id];
            object.set_var("intro", 1.0);
            let d = if object.var("returnWarning") != 0.0 { 128.0 } else { 17.0 };
            object.set_var("dialog", d);
            object.set_var("returnWarning", 0.0);
        }
        return;
    }
    if m.objects[id].pos[1] < m.objects[id].home[1] - 100.0 { begin_return(&mut m.objects[id]); return; }
    if m.flags & MARIO_VANISH_CAP != 0 && m.objects[id].var("grabbing") == 0.0 {
        let object = &mut m.objects[id];
        object.forward_vel = 0.0;
        object.step_physics(&m.world, None);
        return;
    }
    let object = &mut m.objects[id];
    let cd = (0.0f64).max(object.var("grabCooldown") - 1.0);
    object.set_var("grabCooldown", cd);
    if object.distance_to_mario < 2500.0 && object.action == KING_ACT_IDLE {
        object.forward_vel = 3.0;
        object.turn_toward_angle(object.angle_to_mario, 0x100 as f64);
        object.step_physics(&m.world, None);
    } else {
        object.forward_vel = 0.0;
        object.step_physics(&m.world, None);
    }
    object.face_yaw = object.move_yaw;
}
