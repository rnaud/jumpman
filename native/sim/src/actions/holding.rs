//! Port of `src/sim/actions/holding.ts` (picking up, placing down, throwing, Bowser grab).
//! Dispatched from the object group.
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stationary_ground_step;
use crate::math::*;
use crate::object::INT_SUBTYPE_HOLDABLE_NPC;
use crate::state::*;

pub fn act_picking_up(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.drop_held_object();
        return m.set_action(ACT_FREEFALL, 0.0);
    }
    m.action_timer += 1.0;
    m.anim_length = 20.0;
    if m.action_state == 0.0 && m.action_timer >= 5.0 {
        m.grab_used_object();
        m.action_state = 1.0;
    }
    if m.action_state == 1.0 && m.action_timer >= 20.0 {
        return m.set_action(ACT_HOLD_IDLE, 0.0);
    }
    m.step_ground();
    false
}

pub fn act_placing_down(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.drop_held_object();
        return m.set_action(ACT_FREEFALL, 0.0);
    }
    m.action_timer += 1.0;
    m.anim_length = 20.0;
    if m.action_timer == 8.0 { m.drop_held_object(); }
    m.step_ground();
    if m.action_timer >= 20.0 { return m.set_action(ACT_IDLE, 0.0); }
    false
}

pub fn act_throwing(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 {
        m.drop_held_object();
        return m.set_action(ACT_FREEFALL, 0.0);
    }
    if let Some(h) = m.held_obj {
        if m.objects[h].interact_subtype & INT_SUBTYPE_HOLDABLE_NPC != 0 {
            return m.set_action(ACT_PLACING_DOWN, 0.0);
        }
    }
    m.action_timer += 1.0;
    m.anim_length = 12.0;
    if m.action_timer == 7.0 { m.throw_held_object(); }
    m.step_ground();
    if m.action_timer >= 12.0 { return m.set_action(ACT_IDLE, 0.0); }
    false
}

pub fn act_picking_up_bowser(m: &mut MarioRuntime) -> bool {
    if m.action_timer == 0.0 {
        m.angle_vel[1] = 0.0;
        m.grab_used_object();
        m.events.push(MarioEvent::BowserGrab);
    }
    let w = m.world.clone();
    stationary_ground_step(&mut m.state, &w);
    m.action_timer += 1.0;
    m.action_timer >= 20.0 && m.set_action(ACT_HOLDING_BOWSER, 0.0)
}

pub fn act_holding_bowser(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_B_PRESSED != 0 {
        let fast = m.angle_vel[1].abs() >= 0xe00 as f64;
        m.events.push(MarioEvent::BowserThrow { fast });
        return m.set_action(ACT_RELEASING_BOWSER, 0.0);
    }
    if m.angle_vel[1] == 0.0 {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t > 120.0 { return m.set_action(ACT_RELEASING_BOWSER, 1.0); }
    } else {
        m.action_timer = 0.0;
    }
    if m.intended_mag > 20.0 {
        if m.action_arg != 0.0 {
            let spin = clamp((to_s16(m.intended_yaw - m.twirl_yaw) as f64 / 128.0).trunc(), -128.0, 128.0);
            m.angle_vel[1] = clamp(m.angle_vel[1] + spin, -4096.0, 4096.0);
        }
        m.action_arg = 1.0;
        m.twirl_yaw = m.intended_yaw;
    } else {
        m.action_arg = 0.0;
        m.angle_vel[1] = approach_s32(m.angle_vel[1], 0.0, 64.0, 64.0);
    }
    let old_yaw = m.face_angle[1];
    let velocity = m.angle_vel[1];
    m.face_angle[1] = to_s16(old_yaw + velocity) as f64;
    if (velocity >= 0x100 as f64 && old_yaw > m.face_angle[1]) || (velocity <= -0x100 as f64 && old_yaw < m.face_angle[1]) {
        m.events.push(MarioEvent::BowserSpin);
    }
    let w = m.world.clone();
    stationary_ground_step(&mut m.state, &w);
    false
}

pub fn act_releasing_bowser(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    if m.action_timer == 1.0 {
        if let Some(boss) = m.held_obj {
            let pos = m.objects[boss].pos;
            let spin = m.angle_vel[1];
            m.objects[boss].vars.insert("spin".to_string(), spin);
            if m.action_arg != 0.0 { m.drop_held_object(); } else { m.throw_held_object(); }
            m.objects[boss].pos = pos;
            m.angle_vel[1] = 0.0;
        }
    }
    let w = m.world.clone();
    stationary_ground_step(&mut m.state, &w);
    m.action_timer >= 40.0 && m.set_action(ACT_IDLE, 0.0)
}

/// Action table entries (the TS `OBJECT_ACTIONS` rows whose bodies live in holding.ts).
pub fn holding_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_PICKING_UP_BOWSER => act_picking_up_bowser(m),
        ACT_HOLDING_BOWSER => act_holding_bowser(m),
        ACT_RELEASING_BOWSER => act_releasing_bowser(m),
        ACT_PICKING_UP => act_picking_up(m),
        ACT_PLACING_DOWN => act_placing_down(m),
        ACT_THROWING => act_throwing(m),
        _ => return None,
    })
}
