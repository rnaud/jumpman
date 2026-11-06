import { atan2s, clamp, sins, coss, toS16, type Vec3 } from '../math';
import { marioSetForwardVel } from '../marioHelpers';
import type { MarioRuntime } from '../MarioRuntime';
import { ACT_BACKWARD_AIR_KB, ACT_DIVE_SLIDE, ACT_FREEFALL, ACT_FLYING, ACT_LAVA_BOOST,
  ACT_SHOT_FROM_CANNON, AIR_STEP_HIT_LAVA_WALL, AIR_STEP_HIT_WALL, AIR_STEP_LANDED,
  INPUT_A_PRESSED, MARIO_WING_CAP } from '../state';

export interface CannonSeat {
  home: Vec3;
  baseYaw: number;
  yaw: number;
  pitch: number;
  rise: number;
  shake: number;
  inputYaw: number;
  cooldown: number;
}

// opened_cannon_act_4, _6, _5 and act_in_cannon. Timers advance at 30 Hz.
export function inCannon(m: MarioRuntime): boolean {
  const seat = m.cannon;
  if (!seat) return m.setAction(ACT_FREEFALL);
  const t = m.actionTimer++;
  if (m.actionState === 0) {
    m.pos = [seat.home[0], seat.home[1]+10, seat.home[2]];
    m.vel = [0,0,0]; m.forwardVel = 0;
    seat.inputYaw = 0;
    m.actionState = 1;
  }
  if (m.actionState === 1) {
    if (t === 0 || t === 69 || t === 96) m.events.push({type:'cannonMove', phase:t === 0 ? 0 : t === 69 ? 1 : 2});
    if (t < 69) {
      const direction = (t >> 1 & 1) ? 1 : -1;
      seat.shake += direction * (t === 68 ? 3 : 1);
    } else if (t < 73) seat.shake += ((t-69 >> 1 & 1) ? 2 : -2);
    seat.rise = Math.min(69,t+1)*5;
    if (t >= 75 && t < 91) seat.yaw = toS16(seat.baseYaw + sins((t-75)*0x400)*0x4000);
    if (t >= 100 && t < 116) seat.pitch = Math.trunc(sins((t-99)*0x400)*0x2000);
    if (t >= 122) { m.actionState = 2; m.faceAngle = [seat.pitch,seat.yaw,0]; }
    return false;
  }
  const previousPitch = m.faceAngle[0], previousYaw = m.faceAngle[1];
  seat.inputYaw = clamp(seat.inputYaw-Math.trunc(m.controller.stickX*10),-0x4000,0x4000);
  m.faceAngle[0] = clamp(m.faceAngle[0]-Math.trunc(m.controller.stickY*10),0,0x38e3);
  m.faceAngle[1] = toS16(seat.yaw+seat.inputYaw);
  if (!(m.input & INPUT_A_PRESSED)) {
    if (previousPitch !== m.faceAngle[0] || previousYaw !== m.faceAngle[1]) m.events.push({type:'cannonMove',phase:3});
    return false;
  }
  const [pitch,yaw] = m.faceAngle;
  m.forwardVel = 100*coss(pitch);
  m.vel[1] = 100*sins(pitch);
  m.pos[0] += 120*coss(pitch)*sins(yaw);
  m.pos[1] += 120*sins(pitch);
  m.pos[2] += 120*coss(pitch)*coss(yaw);
  seat.cooldown = 6;
  m.cannon = null;
  m.setAction(ACT_SHOT_FROM_CANNON);
  m.events.push({type:'cannonFire'});
  return false;
}

export function shotFromCannon(m: MarioRuntime): boolean {
  marioSetForwardVel(m,m.forwardVel);
  switch (m.stepAir()) {
    case AIR_STEP_LANDED:
      m.faceAngle[0] = 0;
      m.setAction(ACT_DIVE_SLIDE);
      break;
    case AIR_STEP_HIT_WALL:
      marioSetForwardVel(m,-16);
      m.faceAngle[0] = 0;
      m.vel[1] = Math.min(0,m.vel[1]);
      m.setAction(ACT_BACKWARD_AIR_KB);
      break;
    case AIR_STEP_HIT_LAVA_WALL: m.setAction(ACT_LAVA_BOOST); break;
    default: m.faceAngle[0] = atan2s(m.forwardVel,m.vel[1]); break;
  }
  if ((m.flags & MARIO_WING_CAP) && m.vel[1] < 0) m.setAction(ACT_FLYING);
  m.forwardVel = Math.max(10,m.forwardVel-0.05);
  return false;
}
