//! Port of `src/sim/actions/automatic.ts` (ledge grab/climb, hanging, poles; `ACT_IN_CANNON` is
//! routed to cannon.rs by the dispatcher).
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stop_and_set_height_to_floor;
use crate::surfaces::SURFACE_HANGABLE;
use crate::{math::*, state::*};

pub fn sin(angle: f64) -> f64 { js_sin(to_s16(angle) as f64 / 32768.0 * std::f64::consts::PI) }
pub fn cos(angle: f64) -> f64 { js_cos(to_s16(angle) as f64 / 32768.0 * std::f64::consts::PI) }
fn s16(x: f64) -> f64 { to_s16(x) as f64 }

/// let_go_of_ledge
pub fn let_go_of_ledge(m: &mut MarioRuntime) -> bool {
    m.vel[1] = 0.0;
    m.forward_vel = -8.0;
    m.pos[0] -= 60.0 * sin(m.face_angle[1]);
    m.pos[2] -= 60.0 * cos(m.face_angle[1]);

    let floor_hit = m.world.find_floor(m.pos[0], m.pos[1], m.pos[2]);
    if floor_hit.height < m.pos[1] - 100.0 {
        m.pos[1] -= 100.0;
    } else {
        m.pos[1] = floor_hit.height;
        if let Some(s) = floor_hit.surface {
            m.floor = Some(s);
            m.floor_height = floor_hit.height;
        }
    }

    m.set_action(ACT_SOFT_BONK, 0.0)
}

/// act_ledge_grab
pub fn act_ledge_grab(m: &mut MarioRuntime) -> bool {
    let has_space_for_mario = m.ceil_height - m.floor_height >= 160.0;

    if m.action_timer < 10.0 { m.action_timer += 1.0; }

    if m.floor.as_ref().is_some_and(|f| f.normal[1] < 0.9063078) {
        return let_go_of_ledge(m);
    }
    if m.input & (INPUT_Z_PRESSED | INPUT_OFF_FLOOR) != 0 {
        return let_go_of_ledge(m);
    }
    if m.input & INPUT_A_PRESSED != 0 && has_space_for_mario {
        return m.set_action(ACT_LEDGE_CLIMB_FAST, 0.0);
    }

    if m.action_timer == 10.0 && m.input & INPUT_NONZERO_ANALOG != 0 {
        let intended_d_yaw = to_s16(m.intended_yaw - m.face_angle[1]);
        if (-0x4000..=0x4000).contains(&intended_d_yaw) && has_space_for_mario {
            return m.set_action(ACT_LEDGE_CLIMB_SLOW_1, 0.0);
        }
        if !(-0x4000..=0x4000).contains(&intended_d_yaw) {
            return let_go_of_ledge(m);
        }
    }

    let height_above_floor = m.pos[1] - m.floor_height_relative_polar(-0x8000 as f64, 30.0);
    if has_space_for_mario && height_above_floor < 100.0 {
        return m.set_action(ACT_LEDGE_CLIMB_FAST, 0.0);
    }

    stop_and_set_height_to_floor(&mut m.state);
    false
}

/// act_ledge_climb_slow
pub fn act_ledge_climb_slow(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return let_go_of_ledge(m); }

    stop_and_set_height_to_floor(&mut m.state);

    m.action_timer += 1.0;
    if m.action_timer >= 28.0 {
        m.pos[0] += 14.0 * sin(m.face_angle[1]);
        m.pos[2] += 14.0 * cos(m.face_angle[1]);
        return m.set_action(ACT_IDLE, 0.0);
    }
    false
}

/// act_ledge_climb_fast
pub fn act_ledge_climb_fast(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_OFF_FLOOR != 0 { return let_go_of_ledge(m); }

    stop_and_set_height_to_floor(&mut m.state);

    m.action_timer += 1.0;
    if m.action_timer >= 12.0 {
        // climb_up_ledge
        m.pos[0] += 14.0 * sin(m.face_angle[1]);
        m.pos[2] += 14.0 * cos(m.face_angle[1]);
        return m.set_action(ACT_IDLE, 0.0);
    }
    false
}

pub fn can_hang(m: &MarioRuntime) -> bool {
    m.ceil.as_ref().is_some_and(|c| c.kind == SURFACE_HANGABLE)
}

/// act_start_hanging
pub fn act_start_hanging(m: &mut MarioRuntime) -> bool {
    m.action_timer += 1.0;
    if m.input & INPUT_A_DOWN == 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }
    if !can_hang(m) { return m.set_action(ACT_FREEFALL, 0.0); }

    m.anim_length = 53.0;
    m.pos[1] = m.ceil_height - 160.0;
    m.forward_vel = 0.0;
    m.vel = [0.0; 3];
    if m.anim_frame >= 53.0 || m.action_timer >= 54.0 { return m.set_action(ACT_HANGING, 0.0); }
    false
}

/// act_hanging
pub fn act_hanging(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_NONZERO_ANALOG != 0 {
        let arg = m.action_arg;
        return m.set_action(ACT_HANG_MOVING, arg);
    }
    if m.input & INPUT_A_DOWN == 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }
    if !can_hang(m) { return m.set_action(ACT_FREEFALL, 0.0); }

    m.anim_length = 30.0;
    m.pos[1] = m.ceil_height - 160.0;
    m.forward_vel = 0.0;
    m.vel = [0.0; 3];
    false
}

/// act_hang_moving
pub fn act_hang_moving(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_A_DOWN == 0 { return m.set_action(ACT_FREEFALL, 0.0); }
    if m.input & INPUT_Z_PRESSED != 0 { return m.set_action(ACT_GROUND_POUND, 0.0); }
    if !can_hang(m) { return m.set_action(ACT_FREEFALL, 0.0); }

    m.action_timer += 1.0;
    m.anim_length = 30.0;
    m.forward_vel = (m.forward_vel + 1.0).min(4.0);
    m.face_angle[1] = m.intended_yaw;
    m.pos[0] += m.forward_vel * sin(m.face_angle[1]);
    m.pos[2] += m.forward_vel * cos(m.face_angle[1]);
    m.pos[1] = m.ceil_height - 160.0;
    m.vel = [0.0; 3];

    if m.input & INPUT_NONZERO_ANALOG == 0 {
        let arg = ((m.action_arg as i64) ^ 1) as f64;
        return m.set_action(ACT_HANGING, arg);
    }
    false
}

pub fn pole_top(m: &MarioRuntime) -> f64 {
    match m.used_obj { Some(id) => m.objects[id].hitbox.height - 100.0, None => 0.0 }
}

// Root Y translation from Mario's handstand animation (anim_0B_0C), in world units.
const HANDSTAND_HEIGHT: [f64; 17] = [50.0, 61.0, 76.0, 93.0, 107.0, 119.0, 130.0, 140.0, 148.0, 156.0, 162.0, 167.0, 169.0, 171.0, 172.0, 173.0, 173.0];

pub fn set_pole_position(m: &mut MarioRuntime, offset: f64) -> bool {
    let pole = match m.used_obj {
        Some(id) if m.objects[id].active => id,
        _ => { m.set_action(ACT_FREEFALL, 0.0); return false; }
    };
    let (pole_pos, down_offset) = { let p = &m.objects[pole]; (p.pos, p.hitbox.down_offset) };
    m.pole_pos = pole_top(m).min(m.pole_pos);
    m.pos[0] = pole_pos[0];
    m.pos[1] = pole_pos[1] + m.pole_pos + offset;
    m.pos[2] = pole_pos[2];
    let world = m.world.clone();
    let upper_wall = world.f32_find_wall_collision(&mut m.state.pos, 60.0, 50.0);
    let lower_wall = world.f32_find_wall_collision(&mut m.state.pos, 30.0, 24.0);
    let ceiling = world.vec3f_find_ceil(&m.pos, m.pos[1]);
    if m.pos[1] > ceiling.height - 160.0 {
        m.pos[1] = ceiling.height - 160.0;
        m.pole_pos = m.pos[1] - pole_pos[1];
    }
    let floor = world.find_floor(m.pos[0], m.pos[1], m.pos[2]);
    m.floor = floor.surface;
    m.floor_height = floor.height;
    if m.pos[1] < floor.height {
        m.pos[1] = floor.height;
        m.set_action(ACT_IDLE, 0.0);
        return false;
    }
    if m.pole_pos < -down_offset {
        m.pos[1] = pole_pos[1] - down_offset;
        m.set_action(ACT_FREEFALL, 0.0);
        return false;
    }
    if upper_wall != 0 || lower_wall != 0 {
        if m.pos[1] > floor.height + 20.0 {
            m.forward_vel = -2.0;
            m.set_action(ACT_SOFT_BONK, 0.0);
        } else {
            m.set_action(ACT_IDLE, 0.0);
        }
        return false;
    }
    m.forward_vel = 0.0;
    m.vel = [0.0; 3];
    true
}

pub fn leave_pole(m: &mut MarioRuntime, action: u32) -> bool {
    m.face_angle[0] = 0.0;
    m.set_action(action, 0.0)
}

/// act_grab_pole_slow / act_grab_pole_fast
pub fn act_grab_pole(m: &mut MarioRuntime) -> bool {
    if m.action == ACT_GRAB_POLE_FAST {
        m.face_angle[1] = s16(m.face_angle[1] + m.pole_yaw_velocity);
        m.pole_yaw_velocity = (m.pole_yaw_velocity * 8.0 / 10.0).trunc();
        if m.action_state == 0.0 && m.pole_yaw_velocity <= 0x800 as f64 {
            m.action_state = 1.0;
            m.anim_frame = 1.0;
        }
        m.anim_length = if m.action_state != 0.0 { 19.0 } else { 1.0 };
    } else {
        m.anim_length = 11.0;
    }
    if !set_pole_position(m, 0.0) { return true; }
    if (m.action != ACT_GRAB_POLE_FAST || m.action_state != 0.0) && m.anim_frame >= m.anim_length {
        return m.set_action(ACT_HOLDING_POLE, 0.0);
    }
    false
}

/// act_holding_pole
pub fn act_holding_pole(m: &mut MarioRuntime) -> bool {
    if m.input & INPUT_Z_PRESSED != 0 || m.health < 0x100 as f64 {
        m.forward_vel = -2.0;
        return leave_pole(m, ACT_SOFT_BONK);
    }
    if m.input & INPUT_A_PRESSED != 0 {
        m.face_angle[1] = (((m.face_angle[1] + 0x8000 as f64) as i64) & 0xffff) as f64;
        return leave_pole(m, ACT_WALL_KICK_AIR);
    }

    if m.stick_y > 16.0 {
        if m.pole_pos < pole_top(m) - 0.4 { return m.set_action(ACT_CLIMBING_POLE, 0.0); }
        if m.stick_y > 50.0 { return m.set_action(ACT_TOP_OF_POLE_TRANSITION, 0.0); }
    }
    if m.stick_y < -16.0 {
        m.pole_yaw_velocity = (0x1000 as f64).min((m.pole_yaw_velocity - m.stick_y * 2.0).trunc());
        m.face_angle[1] = s16(m.face_angle[1] + m.pole_yaw_velocity);
        m.pole_pos -= (m.pole_yaw_velocity / 0x100 as f64).trunc();
    } else {
        m.pole_yaw_velocity = 0.0;
        m.face_angle[1] = s16(m.face_angle[1] - m.stick_x * 16.0);
    }

    m.anim_length = 30.0;
    if !set_pole_position(m, 0.0) { return true; }
    false
}

/// act_climbing_pole
pub fn act_climbing_pole(m: &mut MarioRuntime) -> bool {
    if m.health < 0x100 as f64 {
        m.forward_vel = -2.0;
        return leave_pole(m, ACT_SOFT_BONK);
    }
    if m.input & INPUT_A_PRESSED != 0 {
        m.face_angle[1] = (((m.face_angle[1] + 0x8000 as f64) as i64) & 0xffff) as f64;
        return leave_pole(m, ACT_WALL_KICK_AIR);
    }
    if m.stick_y < 8.0 { return m.set_action(ACT_HOLDING_POLE, 0.0); }

    m.pole_pos += m.stick_y / 8.0;
    m.pole_yaw_velocity = 0.0;
    m.face_angle[1] = s16(m.camera_yaw - approach_s32(s16(m.camera_yaw - m.face_angle[1]), 0.0, 0x400 as f64, 0x400 as f64));

    m.anim_length = 20.0;
    if !set_pole_position(m, 0.0) { return true; }
    false
}

/// act_top_of_pole_transition
pub fn act_top_of_pole_transition(m: &mut MarioRuntime) -> bool {
    m.pole_pos = pole_top(m);
    m.anim_length = 16.0;
    m.pole_yaw_velocity = 0.0;
    let frame = if m.action_arg != 0.0 { 16.0 - m.anim_frame } else { m.anim_frame };
    let offset = if frame >= 0.0 { HANDSTAND_HEIGHT.get(frame as usize).copied().unwrap_or(f64::NAN) } else { f64::NAN };
    if !set_pole_position(m, offset) { return true; }
    if m.anim_frame >= 16.0 {
        let a = if m.action_arg != 0.0 { ACT_HOLDING_POLE } else { ACT_TOP_OF_POLE };
        return m.set_action(a, 0.0);
    }
    false
}

/// act_top_of_pole
pub fn act_top_of_pole(m: &mut MarioRuntime) -> bool {
    m.pole_pos = pole_top(m);
    if m.input & INPUT_A_PRESSED != 0 { return m.set_action(ACT_TOP_OF_POLE_JUMP, 0.0); }
    if m.stick_y < -16.0 { return m.set_action(ACT_TOP_OF_POLE_TRANSITION, 1.0); }

    m.face_angle[1] = s16(m.face_angle[1] - m.stick_x * 16.0);
    if !set_pole_position(m, 174.0) { return true; }
    m.anim_length = 51.0;
    false
}

/// Action table (`AUTOMATIC_ACTIONS`; `ACT_IN_CANNON` lives in cannon.rs).
pub fn automatic_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_LEDGE_GRAB => act_ledge_grab(m),
        ACT_LEDGE_CLIMB_SLOW_1 | ACT_LEDGE_CLIMB_SLOW_2 => act_ledge_climb_slow(m),
        ACT_LEDGE_CLIMB_FAST => act_ledge_climb_fast(m),
        ACT_START_HANGING => act_start_hanging(m),
        ACT_HANGING => act_hanging(m),
        ACT_HANG_MOVING => act_hang_moving(m),
        ACT_GRAB_POLE_SLOW | ACT_GRAB_POLE_FAST => act_grab_pole(m),
        ACT_HOLDING_POLE => act_holding_pole(m),
        ACT_CLIMBING_POLE => act_climbing_pole(m),
        ACT_TOP_OF_POLE_TRANSITION => act_top_of_pole_transition(m),
        ACT_TOP_OF_POLE => act_top_of_pole(m),
        _ => return None,
    })
}
