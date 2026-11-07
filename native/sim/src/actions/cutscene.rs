//! Port of `src/sim/actions/cutscene.ts` (deaths, spawn/exit airborne, star dance, signs;
//! `ACT_END_PEACH_CUTSCENE` / `ACT_JUMBO_STAR_CUTSCENE` are handled by ending.rs, which the dispatcher also tries).
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
use crate::mario_step::stop_and_set_height_to_floor;
use crate::math::*;
use crate::state::*;

fn death_frame(m: &mut MarioRuntime, frames: f64) -> bool {
    stop_and_set_height_to_floor(m);
    m.anim_length = frames;
    m.action_timer += 1.0;
    if m.action_timer >= frames {
        m.respawn(false);
    }
    false
}

pub fn act_quicksand_death(m: &mut MarioRuntime) -> bool {
    stop_and_set_height_to_floor(m);
    m.anim_length = 30.0;
    m.quicksand_depth += 5.0;
    if m.quicksand_depth >= 180.0 { m.respawn(false); }
    false
}

pub fn act_standing_death(m: &mut MarioRuntime) -> bool { death_frame(m, 80.0) }
pub fn act_death_on_back(m: &mut MarioRuntime) -> bool { death_frame(m, 54.0) }
pub fn act_death_on_stomach(m: &mut MarioRuntime) -> bool { death_frame(m, 37.0) }

pub fn spawn_spin_airborne(m: &mut MarioRuntime) -> bool {
    let fv = m.forward_vel;
    mario_set_forward_vel(m, fv);
    if m.step_air(0) == AIR_STEP_LANDED as i32 {
        m.set_action(ACT_SPAWN_SPIN_LANDING, 0.0);
    } else if m.pos[1] - m.floor_height <= 300.0 {
        m.action_state = 1.0;
    }
    false
}

pub fn star_exit_airborne(m: &mut MarioRuntime) -> bool {
    let painting = m.action == ACT_EXIT_AIRBORNE;
    let launch = m.action == ACT_SPECIAL_EXIT_AIRBORNE;
    if painting {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t <= 15.0 { return false; }
    } else if launch {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t < 11.0 { return false; }
    }
    mario_set_forward_vel(m, if painting { -32.0 } else if launch { -24.0 } else { 0.0 });
    if m.step_air(0) == AIR_STEP_LANDED as i32 {
        m.set_action(ACT_EXIT_LAND_SAVE_DIALOG, if launch { 1.0 } else { 0.0 });
        m.heal_counter = 31.0;
    }
    false
}

pub fn death_exit(m: &mut MarioRuntime) -> bool {
    let painting = m.action == ACT_DEATH_EXIT;
    let launch = m.action == ACT_SPECIAL_DEATH_EXIT;
    m.health = 0x100 as f64;
    if painting {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t <= 15.0 { return false; }
    } else if launch {
        let t = m.action_timer;
        m.action_timer += 1.0;
        if t < 11.0 { return false; }
    }
    mario_set_forward_vel(m, if painting { -32.0 } else if launch { -24.0 } else { 0.0 });
    if m.step_air(0) == AIR_STEP_LANDED as i32 {
        let landing = if launch {
            ACT_HARD_BACKWARD_GROUND_KB
        } else if m.action == ACT_UNUSED_DEATH_EXIT {
            ACT_FREEFALL_LAND_STOP
        } else {
            ACT_DEATH_EXIT_LAND
        };
        m.set_action(landing, 0.0);
        m.lives -= 1.0;
        m.heal_counter = 31.0;
        if !launch { m.events.push(MarioEvent::DeathLanding); }
    }
    false
}

pub fn warp_landing(m: &mut MarioRuntime) -> bool {
    stop_and_set_height_to_floor(m);
    // GENERAL_LAND plays source frames 9 through 23.
    m.action_timer += 1.0;
    if m.action_timer >= 15.0 { m.set_action(ACT_IDLE, 0.0); }
    false
}

pub fn ready_for_npc_dialog(m: &MarioRuntime) -> bool {
    let group = m.action & ACT_GROUP_MASK;
    (group == ACT_GROUP_STATIONARY || group == ACT_GROUP_MOVING)
        && m.action & (ACT_FLAG_RIDING_SHELL | ACT_FLAG_INVULNERABLE) == 0
}

pub fn reading_npc_dialog(m: &mut MarioRuntime) -> bool {
    let head_turn = if m.action_arg == 2.0 { -1024.0 } else if m.action_arg == 3.0 { 384.0 } else { 0.0 };
    if m.action_state < 8.0 && m.used_obj.is_some() {
        let o = &m.objects[m.used_obj.unwrap()];
        let target = atan2s(o.pos[2] - m.pos[2], o.pos[0] - m.pos[0]) as f64;
        let d = clamp(to_s16(target - m.face_angle[1]) as f64, -2048.0, 2048.0);
        m.face_angle[1] = to_s16(m.face_angle[1] + d) as f64;
        m.action_timer += head_turn;
    } else if m.action_state >= 9.0 && m.action_state < 17.0 {
        m.action_timer -= head_turn;
    } else if m.action_state == 23.0 {
        m.set_action(if m.held_obj.is_some() { ACT_HOLD_IDLE } else { ACT_IDLE }, 0.0);
        return false;
    }
    if m.action_state != 8.0 { m.action_state += 1.0; }
    false
}

pub fn ground_star_dance(m: &mut MarioRuntime) -> bool {
    stop_and_set_height_to_floor(m);
    m.face_angle[1] = m.camera_yaw;
    m.action_timer += 1.0;
    false
}

/// Action table (`CUTSCENE_ACTIONS`, minus the two ending actions).
pub fn cutscene_action(m: &mut MarioRuntime) -> Option<bool> {
    Some(match m.action {
        ACT_FALL_AFTER_STAR_GRAB => {
            if m.pos[1] < m.world.water_level(m.pos[0], m.pos[2]) - 130.0 {
                let arg = m.action_arg;
                return Some(m.set_action(ACT_STAR_DANCE_WATER, arg));
            }
            if m.step_air(1) == AIR_STEP_LANDED as i32 {
                let arg = m.action_arg;
                m.set_action(if arg != 0.0 { ACT_STAR_DANCE_NO_EXIT } else { ACT_STAR_DANCE_EXIT }, arg);
            }
            false
        }
        ACT_STAR_DANCE_EXIT | ACT_STAR_DANCE_NO_EXIT => ground_star_dance(m),
        ACT_STAR_DANCE_WATER => {
            m.face_angle[1] = m.camera_yaw;
            m.action_timer += 1.0;
            false
        }
        ACT_READING_SIGN => {
            if m.action_state != 2.0 {
                m.action_state = 1.0;
                m.pos[0] += m.sign_motion[0];
                m.face_angle[1] = to_s16(m.face_angle[1] + m.sign_motion[1]) as f64;
                m.pos[2] += m.sign_motion[2];
                m.action_timer += 1.0;
                if m.action_timer == 11.0 { m.action_state = 2.0; }
            } else {
                m.action_timer += 1.0;
            }
            false
        }
        ACT_READING_NPC_DIALOG => reading_npc_dialog(m),
        ACT_SPAWN_NO_SPIN_AIRBORNE => {
            mario_set_forward_vel(m, 0.0);
            if m.step_air(0) == AIR_STEP_LANDED as i32 { m.set_action(ACT_SPAWN_NO_SPIN_LANDING, 0.0); }
            false
        }
        ACT_SPAWN_NO_SPIN_LANDING | ACT_SPAWN_SPIN_LANDING | ACT_EXIT_LAND_SAVE_DIALOG => warp_landing(m),
        ACT_SPAWN_SPIN_AIRBORNE => spawn_spin_airborne(m),
        ACT_SPECIAL_EXIT_AIRBORNE | ACT_EXIT_AIRBORNE | ACT_FALLING_EXIT_AIRBORNE => star_exit_airborne(m),
        ACT_DEATH_EXIT | ACT_FALLING_DEATH_EXIT | ACT_SPECIAL_DEATH_EXIT | ACT_UNUSED_DEATH_EXIT => death_exit(m),
        ACT_STANDING_DEATH => act_standing_death(m),
        ACT_TELEPORT_FADE_OUT => {
            m.action_timer += 1.0;
            stop_and_set_height_to_floor(m);
            false
        }
        ACT_TELEPORT_FADE_IN => {
            let t = m.action_timer;
            m.action_timer += 1.0;
            if t == 32.0 {
                let a = if m.pos[1] < m.world.water_level(m.pos[0], m.pos[2]) - 100.0 { ACT_WATER_IDLE } else { ACT_IDLE };
                m.set_action(a, 0.0);
            }
            stop_and_set_height_to_floor(m);
            false
        }
        ACT_DEATH_ON_BACK => act_death_on_back(m),
        ACT_DEATH_ON_STOMACH => act_death_on_stomach(m),
        ACT_QUICKSAND_DEATH => act_quicksand_death(m),
        _ => return None,
    })
}
