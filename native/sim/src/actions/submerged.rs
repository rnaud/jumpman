//! Port of `src/sim/actions/submerged.ts` (normal-cap swimming, `submergedStep`).
use crate::math::*;
use crate::mario_runtime::MarioRuntime;
use crate::state::*;

/// Normal-cap swimming. `surface` is the water surface height at Mario's x/z.
pub fn submerged_step(m: &mut MarioRuntime, surface: f64) {
    if surface < m.floor_height + 80.0 {
        m.face_angle[0] = 0.0;
        let a = if m.pos[1] > m.floor_height + 100.0 { ACT_FREEFALL } else { ACT_WALKING };
        m.set_action(a, 0.0);
        return;
    }
    let pressed = m.input & INPUT_A_PRESSED != 0;
    let held = m.input & INPUT_A_DOWN != 0;
    let near_surface = surface - 80.0 - m.pos[1] < 400.0;
    let buoyancy = if near_surface { 1.25 } else if m.action & ACT_FLAG_STATIONARY != 0 { -2.0 } else { 0.0 };
    // Transitions execute the new action during the same tick, as in mario_execute_submerged_action.
    for _ in 0..4 {
        let before = m.action;
        if (before == ACT_BREASTSTROKE || before == ACT_SWIMMING_END) && pressed
            && (m.pos[1] + 1.5).trunc() >= surface - 80.0 && m.face_angle[0] >= 0.0 && m.controller.stick_y < -60.0 {
            m.swim_yaw_vel = 0.0;
            m.set_action(ACT_WATER_JUMP, 0.0);
            m.vel[1] = 42.0;
            return;
        }
        match before {
            ACT_WATER_IDLE | ACT_WATER_ACTION_END => {
                if pressed {
                    m.set_action(ACT_BREASTSTROKE, 0.0);
                } else if before == ACT_WATER_ACTION_END && { m.action_timer += 1.0; m.action_timer >= 25.0 } {
                    m.set_action(ACT_WATER_IDLE, 0.0);
                }
            }
            ACT_BREASTSTROKE => {
                if m.action_arg == 0.0 { m.swim_strength = 160.0; }
                m.action_timer += 1.0;
                if m.action_timer == 14.0 {
                    m.set_action(ACT_FLUTTER_KICK, 0.0);
                } else {
                    if m.action_timer < 6.0 { m.forward_vel += 0.5; }
                    if m.action_timer >= 9.0 { m.forward_vel += 1.5; }
                    if m.action_timer >= 2.0 && m.action_timer < 6.0 && pressed { m.action_state = 1.0; }
                    if m.action_timer == 9.0 && m.action_state != 0.0 {
                        m.action_timer = 1.0; m.action_state = 0.0; m.swim_strength = 160.0;
                    }
                    if m.action_timer == 1.0 {
                        let fast = m.swim_strength != 160.0;
                        m.events.push(MarioEvent::SwimStroke { fast });
                    }
                }
            }
            ACT_FLUTTER_KICK => {
                if !held {
                    if m.action_timer == 0.0 { m.swim_strength = (m.swim_strength + 10.0).min(280.0); }
                    m.set_action(ACT_SWIMMING_END, 0.0);
                } else {
                    m.forward_vel = approach_f32(m.forward_vel, 12.0, 0.1, 0.15);
                    m.action_timer = 1.0; m.swim_strength = 160.0;
                    if m.forward_vel < 14.0 {
                        if m.anim_frame == 0.0 || m.anim_frame == 12.0 { m.events.push(MarioEvent::FlutterKick); }
                        m.anim_frame = (m.anim_frame + 1.0) % 24.0;
                    }
                }
            }
            ACT_SWIMMING_END => {
                if m.action_timer >= 15.0 {
                    m.set_action(ACT_WATER_ACTION_END, 0.0);
                } else if held && m.action_timer >= 7.0 {
                    if m.action_timer == 7.0 { m.swim_strength = (m.swim_strength + 10.0).min(280.0); }
                    m.set_action(ACT_BREASTSTROKE, 1.0);
                } else {
                    if m.action_timer >= 7.0 { m.swim_strength = 160.0; }
                    m.action_timer += 1.0; m.forward_vel -= 0.25;
                }
            }
            _ => {}
        }
        if m.action == before { break; }
    }
    if m.action == ACT_WATER_PLUNGE {
        let first = m.action_timer == 0.0;
        m.action_timer += 1.0;
        if first { m.events.push(MarioEvent::WaterPlunge); }
        m.swim_yaw_vel = 0.0;
        m.face_angle[0] = approach_f32(m.face_angle[0], 0.0, 0x200 as f64, 0x200 as f64);
        m.forward_vel = approach_f32(m.forward_vel, 0.0, 1.0, 1.0);
        m.vel[1] = approach_f32(m.vel[1], buoyancy, 2.0, 1.0);
    } else {
        let target_yaw = -(10.0 * m.controller.stick_x).trunc();
        if target_yaw > 0.0 && m.swim_yaw_vel < 0.0 {
            m.swim_yaw_vel = (m.swim_yaw_vel + 0x40 as f64).min(0x10 as f64);
        } else if target_yaw < 0.0 && m.swim_yaw_vel > 0.0 {
            m.swim_yaw_vel = (m.swim_yaw_vel - 0x40 as f64).max(-0x10 as f64);
        } else {
            let inc = if target_yaw == 0.0 { 64.0 } else if target_yaw > 0.0 { 16.0 } else { 32.0 };
            let dec = if target_yaw == 0.0 { 64.0 } else if target_yaw > 0.0 { 32.0 } else { 16.0 };
            m.swim_yaw_vel = approach_f32(m.swim_yaw_vel, target_yaw, inc, dec);
        }
        m.face_angle[1] = to_s16(m.face_angle[1] + m.swim_yaw_vel) as f64;
        m.face_angle[2] = -m.swim_yaw_vel * 8.0;
        let pitch_vel = if m.face_angle[0] < 0.0 { 0x100 } else { 0x200 } as f64;
        let target_pitch = -(m.controller.stick_y * 252.0).trunc();
        m.face_angle[0] = approach_f32(m.face_angle[0], target_pitch, pitch_vel, pitch_vel);
        let idle = m.action & ACT_FLAG_STATIONARY != 0;
        m.forward_vel = clamp(m.forward_vel - if idle { 2.0 } else { 0.0 }, 0.0, 28.0);
        if m.forward_vel > if idle { 16.0 } else { m.swim_strength / 10.0 } { m.forward_vel -= 0.5; }
        m.vel[1] = m.forward_vel * sins(m.face_angle[0]) + buoyancy;
    }
    let horizontal = m.forward_vel * coss(m.face_angle[0]);
    m.vel[0] = horizontal * sins(m.face_angle[1]);
    m.vel[2] = horizontal * coss(m.face_angle[1]);
    let mut next: Vec3 = [m.pos[0] + m.vel[0], (surface - 80.0).min(m.pos[1] + m.vel[1]), m.pos[2] + m.vel[2]];
    let world = m.world.clone();
    world.resolve_and_return_wall_collisions(&mut next, 10.0, 110.0);
    let floor = world.find_floor(next[0], next[1], next[2]);
    if let Some(surf) = floor.surface {
        let ceil = world.vec3f_find_ceil(&next, floor.height);
        if ceil.height - floor.height >= 160.0 {
            next[1] = clamp(next[1], floor.height, ceil.height - 160.0);
            m.pos = next; m.floor = Some(surf); m.floor_height = floor.height;
        }
    }
    if m.action == ACT_WATER_PLUNGE
        && (m.pos[1] == m.floor_height || m.vel[1] >= if near_surface { 0.0 } else { -5.0 } || m.action_timer > 20.0) {
        m.set_action(if held { ACT_FLUTTER_KICK } else { ACT_WATER_ACTION_END }, 0.0);
    }
}
