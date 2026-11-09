//! Mario actions the round-2 actors need (SM64 `ACT_TORNADO_TWIRLING`, `ACT_TWIRLING`,
//! `ACT_RIDING_HOOT`): `mario_actions_automatic.c` `act_tornado_twirling`, `mario_actions_airborne.c`
//! `act_twirling` / `act_riding_hoot`. Chained after the other handlers of their group in
//! `actions::execute_mario_action`.
//!
//! Twirling keeps its yaw in `angle_vel[0]` (`m->twirlYaw`) and `angle_vel[1]` (`m->angleVel[1]`);
//! the Tweester's per-Mario fields (`oMarioTornadoPosY`, `oMarioTornadoYawVel`) live in the tornado's vars.
//! Left out: sounds, the twirl landing action (`ACT_TWIRL_LAND` falls back to the freefall landing).
use crate::mario_runtime::MarioRuntime;
use crate::math::*;
use crate::state::*;

const STEP_LANDED: i32 = AIR_STEP_LANDED as i32;
const STEP_HIT_WALL: i32 = AIR_STEP_HIT_WALL as i32;

pub const ACT_TWIRLING: u32 = 0x108008A4;
pub const ACT_TORNADO_TWIRLING: u32 = 0x10020372;
pub const ACT_RIDING_HOOT: u32 = 0x000004A8;
pub const ACT_GETTING_BLOWN: u32 = 0x010208B8;
pub const ACT_THROWN_FORWARD: u32 = 0x010208BD;
pub const ACT_THROWN_BACKWARD: u32 = 0x010208BE;

/// `act_tornado_twirling` (mario_actions_automatic.c).
fn act_tornado_twirling(m: &mut MarioRuntime) -> bool {
    let Some(used) = m.used_obj else { return m.set_action(ACT_FREEFALL, 0.0) };
    let (upos, uheight) = (m.objects[used].pos, m.objects[used].hitbox.height);
    let dx = (m.pos[0] - upos[0]) * 0.95;
    let dz = (m.pos[2] - upos[2]) * 0.95;

    if m.vel[1] < 60.0 {
        m.state.vel[1] += 1.0;
    }
    let mut pos_y = m.objects[used].var("marioPosY") + m.vel[1];
    if pos_y < 0.0 {
        pos_y = 0.0;
    }
    m.objects[used].set_var("marioPosY", pos_y);
    if pos_y > uheight {
        if m.vel[1] < 20.0 {
            m.state.vel[1] = 20.0;
        }
        return m.set_action(ACT_TWIRLING, 1.0);
    }
    if m.angle_vel[1] < 0x3000 as f64 {
        m.state.angle_vel[1] += 0x100 as f64;
    }
    let mut yaw_vel = m.objects[used].var("marioYawVel");
    if yaw_vel < 0x1000 as f64 {
        yaw_vel += 0x100 as f64;
    }
    m.objects[used].set_var("marioYawVel", yaw_vel);
    m.state.angle_vel[0] = to_s16(m.angle_vel[0] + m.angle_vel[1]) as f64;

    let (s, c) = (sins(yaw_vel), coss(yaw_vel));
    let mut next = [upos[0] + dx * c + dz * s, upos[1] + pos_y, upos[2] - dx * s + dz * c];
    m.world.f32_find_wall_collision(&mut next, 60.0, 50.0);
    let floor = m.world.find_floor(next[0], next[1], next[2]);
    if floor.surface.is_some() {
        m.state.pos = next;
    } else if next[1] >= m.floor_height {
        m.state.pos[1] = next[1];
    } else {
        m.state.pos[1] = m.floor_height;
    }
    m.action_timer += 1.0;
    if m.action_timer >= 5.0 {
        m.action_arg = 1.0;
    }
    false
}

/// `act_twirling` (mario_actions_airborne.c): the spin after the tornado throws Mario out.
fn act_twirling(m: &mut MarioRuntime) -> bool {
    let target = if m.input & INPUT_A_DOWN != 0 { 0x2000 as f64 } else { 0x1800 as f64 };
    let mut v = m.angle_vel[1];
    // approach_s32(angleVel, target, 0x200, 0x200)
    if v < target {
        v = (v + 0x200 as f64).min(target);
    } else {
        v = (v - 0x200 as f64).max(target);
    }
    m.state.angle_vel[1] = v;
    m.state.angle_vel[0] = to_s16(m.angle_vel[0] + v) as f64;
    m.action_timer += 1.0;
    if m.action_timer >= 5.0 {
        m.action_arg = 1.0;
    }
    match m.step_air(0) {
        STEP_LANDED => {
            m.state.angle_vel = [0.0; 3];
            m.set_action(ACT_FREEFALL_LAND_STOP, 0.0);
        }
        STEP_HIT_WALL => {
            m.state.forward_vel = -16.0;
            m.set_action(ACT_SOFT_BONK, 0.0);
        }
        _ => {}
    }
    false
}

/// `act_riding_hoot` (mario_actions_airborne.c): Mario hangs under the owl until A is released or
/// the owl lets go (`INT_STATUS_MARIO_UNK7`, here the hoot's `release` var).
fn act_riding_hoot(m: &mut MarioRuntime) -> bool {
    let Some(hoot) = m.used_obj else { return m.set_action(ACT_FREEFALL, 0.0) };
    if m.input & INPUT_A_DOWN == 0 || m.objects[hoot].var("release") != 0.0 {
        let now = m.objects[hoot].var("age");
        let o = &mut m.objects[hoot];
        o.interact_status = 0;
        o.set_var("releaseTime", now);
        o.set_var("release", 0.0);
        return m.set_action(ACT_FREEFALL, 0.0);
    }
    let (hpos, hyaw) = (m.objects[hoot].pos, m.objects[hoot].move_yaw);
    m.state.pos = [hpos[0], hpos[1] - 92.5, hpos[2]];
    m.state.face_angle[1] = to_s16(0x4000 as f64 - hyaw) as f64;
    if m.action_state == 0.0 {
        m.anim_length = 54.0; // MARIO_ANIM_HANG_ON_CEILING, then HANG_ON_OWL
        m.action_timer += 1.0;
        if m.action_timer >= 54.0 {
            m.action_state = 1.0;
        }
    }
    m.state.vel = [0.0; 3];
    false
}

/// Handler table: chained after the group's own handlers by `actions::execute_mario_action`.
pub fn r2a_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        ACT_TORNADO_TWIRLING => Some(act_tornado_twirling(m)),
        ACT_TWIRLING => Some(act_twirling(m)),
        ACT_RIDING_HOOT => Some(act_riding_hoot(m)),
        ACT_GETTING_BLOWN => Some(act_getting_blown(m)),
        ACT_THROWN_FORWARD => Some(act_thrown(m, false)),
        ACT_THROWN_BACKWARD => Some(act_thrown(m, true)),
        _ => None,
    }
}

/// `interact_tornado` (interaction.c): the Tweester lifts Mario into `ACT_TORNADO_TWIRLING`.
pub fn interact_tornado(id: crate::object::ObjId, m: &mut MarioRuntime) -> bool {
    if m.action != ACT_TORNADO_TWIRLING && m.action != ACT_SQUISHED {
        let was_twirling = m.action == ACT_TWIRLING;
        stop_riding_and_holding(m);
        m.state.forward_vel = 0.0;
        m.objects[id].interact_status = crate::object::INT_STATUS_INTERACTED;
        m.interact_obj = Some(id);
        m.used_obj = Some(id);
        let y = m.pos[1] - m.objects[id].pos[1];
        let o = &mut m.objects[id];
        o.set_var("marioYawVel", 0x400 as f64);
        o.set_var("marioPosY", y);
        m.state.angle_vel = [0.0; 3];
        m.set_action(ACT_TORNADO_TWIRLING, was_twirling as i32 as f64);
        return true;
    }
    false
}

/// `interact_hoot`: grab on from an airborne / moving action.
pub fn interact_hoot(id: crate::object::ObjId, m: &mut MarioRuntime) -> bool {
    let action_id = m.action & 0x1FF;
    let age = m.objects[id].var("age");
    if (0x080..0x098).contains(&action_id) && age - m.objects[id].var("releaseTime") > 30.0 {
        stop_riding_and_holding(m);
        m.objects[id].interact_status = 1;
        m.interact_obj = Some(id);
        m.used_obj = Some(id);
        m.set_action(ACT_RIDING_HOOT, 0.0);
        return true;
    }
    false
}

/// `mario_stop_riding_and_holding`.
fn stop_riding_and_holding(m: &mut MarioRuntime) {
    m.drop_held_object();
    m.riding_shell = None;
}

/// `act_thrown_forward` / `act_thrown_backward` (mario_actions_airborne.c): the knockback air step at the speed Mario was
/// thrown with, which decays by 2% a frame; landing is a (hard, when `actionArg`) ground knockback whose own arg is
/// `hurtCounter`, i.e. 0.
fn act_thrown(m: &mut MarioRuntime, backward: bool) -> bool {
    let hard = m.action_arg != 0.0;
    let land = match (backward, hard) {
        (true, true) => ACT_HARD_BACKWARD_GROUND_KB,
        (true, false) => ACT_BACKWARD_GROUND_KB,
        (false, true) => ACT_HARD_FORWARD_GROUND_KB,
        (false, false) => ACT_FORWARD_GROUND_KB,
    };
    let speed = m.forward_vel;
    let r = crate::actions::airborne::common_air_knockback_step(m, land, speed);
    if r == STEP_LANDED {
        m.state.action_arg = 0.0;
    } else {
        m.state.forward_vel *= 0.98;
    }
    false
}

/// `interact_strong_wind` (interaction.c).
pub fn interact_strong_wind(id: crate::object::ObjId, m: &mut MarioRuntime) -> bool {
    if m.action != ACT_GETTING_BLOWN {
        stop_riding_and_holding(m);
        m.objects[id].interact_status = crate::object::INT_STATUS_INTERACTED;
        m.interact_obj = Some(id);
        m.used_obj = Some(id);
        m.state.face_angle[1] = to_s16(m.objects[id].move_yaw + 0x8000 as f64) as f64;
        m.state.getting_blown_gravity = 0.4;
        m.state.forward_vel = -24.0;
        m.state.vel[1] = 12.0;
        m.set_action(ACT_GETTING_BLOWN, 0.0);
    }
    true
}

/// `act_getting_blown` (mario_actions_airborne.c): pushed back at up to 60, then eased to 16; the cap blow-off at frame 20 is left out.
fn act_getting_blown(m: &mut MarioRuntime) -> bool {
    if m.action_state == 0.0 {
        if m.forward_vel > -60.0 {
            m.state.forward_vel -= 6.0;
        } else {
            m.state.action_state = 1.0;
        }
    } else {
        if m.forward_vel < -16.0 {
            m.state.forward_vel += 0.8;
        }
        if m.vel[1] < 0.0 && m.getting_blown_gravity < 4.0 {
            m.state.getting_blown_gravity += 0.05;
        }
    }
    m.state.action_timer += 1.0;
    let speed = m.forward_vel;
    m.state.forward_vel = speed;
    match m.step_air(0) {
        STEP_LANDED => {
            m.set_action(ACT_HARD_BACKWARD_AIR_KB, 0.0);
        }
        STEP_HIT_WALL => {
            m.bonk_reflection(false);
            if m.vel[1] > 0.0 {
                m.state.vel[1] = 0.0;
            }
            m.state.forward_vel = -m.forward_vel;
        }
        _ => {}
    }
    false
}
