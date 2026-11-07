//! Port of `src/sim/actions/cannon.ts` (`ACT_IN_CANNON` and `ACT_SHOT_FROM_CANNON`).
use crate::mario_helpers::mario_set_forward_vel;
use crate::mario_runtime::MarioRuntime;
#[allow(unused_imports)]
use crate::{math::*, state::*};

/// The cannon aiming state (TS `CannonSeat`). Defined here because `MarioRuntime::cannon` holds it.
#[derive(Clone, Debug, Default)]
pub struct CannonSeat {
    pub home: Vec3,
    pub base_yaw: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub rise: f64,
    pub shake: f64,
    pub input_yaw: f64,
    pub cooldown: f64,
}

/// opened_cannon_act_4, _6, _5 and act_in_cannon. Timers advance at 30 Hz.
///
/// The TS shares the seat object with the game (which reads `rise`/`shake`/`cooldown`); here
/// `m.cannon` owns it, so the host must read the seat from `m.cannon` each frame and, when it
/// goes `Some -> None` on `MarioEvent::CannonFire`, start its own 6-frame cooldown.
pub fn in_cannon(m: &mut MarioRuntime) -> bool {
    let Some(mut seat) = m.cannon.take() else { return m.set_action(ACT_FREEFALL, 0.0) };
    let t = m.action_timer;
    m.action_timer += 1.0;
    if m.action_state == 0.0 {
        m.pos = [seat.home[0], seat.home[1] + 10.0, seat.home[2]];
        m.vel = [0.0, 0.0, 0.0];
        m.forward_vel = 0.0;
        seat.input_yaw = 0.0;
        m.action_state = 1.0;
    }
    if m.action_state == 1.0 {
        let ti = t as i32;
        if ti == 0 || ti == 69 || ti == 96 {
            m.events.push(MarioEvent::CannonMove { phase: if ti == 0 { 0 } else if ti == 69 { 1 } else { 2 } });
        }
        if ti < 69 {
            let direction = if (ti >> 1) & 1 != 0 { 1.0 } else { -1.0 };
            seat.shake += direction * if ti == 68 { 3.0 } else { 1.0 };
        } else if ti < 73 {
            seat.shake += if ((ti - 69) >> 1) & 1 != 0 { 2.0 } else { -2.0 };
        }
        seat.rise = (69.0f64).min(t + 1.0) * 5.0;
        if t >= 75.0 && t < 91.0 {
            seat.yaw = to_s16(seat.base_yaw + sins((t - 75.0) * 0x400 as f64) * 0x4000 as f64) as f64;
        }
        if t >= 100.0 && t < 116.0 {
            seat.pitch = (sins((t - 99.0) * 0x400 as f64) * 0x2000 as f64).trunc();
        }
        if t >= 122.0 {
            m.action_state = 2.0;
            m.face_angle = [seat.pitch, seat.yaw, 0.0];
        }
        m.cannon = Some(seat);
        return false;
    }
    let previous_pitch = m.face_angle[0];
    let previous_yaw = m.face_angle[1];
    seat.input_yaw = clamp(seat.input_yaw - (m.controller.stick_x * 10.0).trunc(), -0x4000 as f64, 0x4000 as f64);
    m.face_angle[0] = clamp(m.face_angle[0] - (m.controller.stick_y * 10.0).trunc(), 0.0, 0x38e3 as f64);
    m.face_angle[1] = to_s16(seat.yaw + seat.input_yaw) as f64;
    if m.input & INPUT_A_PRESSED == 0 {
        if previous_pitch != m.face_angle[0] || previous_yaw != m.face_angle[1] {
            m.events.push(MarioEvent::CannonMove { phase: 3 });
        }
        m.cannon = Some(seat);
        return false;
    }
    let (pitch, yaw) = (m.face_angle[0], m.face_angle[1]);
    m.forward_vel = 100.0 * coss(pitch);
    m.vel[1] = 100.0 * sins(pitch);
    m.pos[0] += 120.0 * coss(pitch) * sins(yaw);
    m.pos[1] += 120.0 * sins(pitch);
    m.pos[2] += 120.0 * coss(pitch) * coss(yaw);
    // seat.cooldown = 6 and m.cannon = null (the seat is dropped; see the doc comment above).
    m.cannon = None;
    m.set_action(ACT_SHOT_FROM_CANNON, 0.0);
    m.events.push(MarioEvent::CannonFire);
    false
}

pub fn shot_from_cannon(m: &mut MarioRuntime) -> bool {
    let v = m.forward_vel;
    mario_set_forward_vel(&mut m.state, v);
    match m.step_air(0) as u32 {
        AIR_STEP_LANDED => {
            m.face_angle[0] = 0.0;
            m.set_action(ACT_DIVE_SLIDE, 0.0);
        }
        AIR_STEP_HIT_WALL => {
            mario_set_forward_vel(&mut m.state, -16.0);
            m.face_angle[0] = 0.0;
            m.vel[1] = (0.0f64).min(m.vel[1]);
            m.set_action(ACT_BACKWARD_AIR_KB, 0.0);
        }
        AIR_STEP_HIT_LAVA_WALL => { m.set_action(ACT_LAVA_BOOST, 0.0); }
        _ => { m.face_angle[0] = atan2s(m.forward_vel, m.vel[1]) as f64; }
    }
    if m.flags & MARIO_WING_CAP != 0 && m.vel[1] < 0.0 { m.set_action(ACT_FLYING, 0.0); }
    m.forward_vel = (10.0f64).max(m.forward_vel - 0.05);
    false
}

/// Action table: `ACT_IN_CANNON` (AUTOMATIC_ACTIONS) and `ACT_SHOT_FROM_CANNON` (AIRBORNE_ACTIONS).
pub fn cannon_action(m: &mut MarioRuntime) -> Option<bool> {
    match m.action {
        ACT_IN_CANNON => Some(in_cannon(m)),
        ACT_SHOT_FROM_CANNON => Some(shot_from_cannon(m)),
        _ => None,
    }
}
