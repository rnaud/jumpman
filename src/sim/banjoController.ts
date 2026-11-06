import type { MarioRuntime } from './MarioRuntime';
import { approachF32, clamp, coss, sins, toS16 } from './math';
import {
  ACT_BACKFLIP, ACT_DOUBLE_JUMP, ACT_FLAG_AIR, ACT_FREEFALL, ACT_FREEFALL_LAND, ACT_GROUND_POUND, ACT_JUMP,
  ACT_LAVA_BOOST, ACT_LEDGE_GRAB, ACT_LONG_JUMP, ACT_SIDE_FLIP, ACT_STEEP_JUMP, ACT_TRIPLE_JUMP, ACT_WALL_KICK_AIR,
  A_BUTTON, AIR_STEP_CHECK_LEDGE_GRAB, AIR_STEP_GRABBED_LEDGE, AIR_STEP_HIT_LAVA_WALL, AIR_STEP_LANDED,
  MARIO_WING_CAP, Z_TRIG,
} from './state';

/** ACT_FREEFALL argument marking Kazooie's Feathery Flap. */
export const BANJO_FLAP = 0x60;
export const banjoFlapping = (m: MarioRuntime) => m.action === ACT_FREEFALL && m.actionArg === BANJO_FLAP;

// bsbflap.c, converted from units/second to 30 Hz frames: flap impulse 280, gravity -1100,
// terminal velocity -399.9, and a 2.5 second batimer before Kazooie tires.
const FLAP_IMPULSE = 280 / 30;
const FLAP_GRAVITY = 1100 / 900;
const FLAP_TERMINAL = -399.9 / 30;
const FLAP_FRAMES = 75;
const FLAP_PERIOD = 9;
const FLAP_SPEED = 18;

const FLAP_FROM = new Set([ACT_JUMP, ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_FREEFALL, ACT_SIDE_FLIP,
  ACT_BACKFLIP, ACT_STEEP_JUMP, ACT_WALL_KICK_AIR, ACT_LONG_JUMP]);

/**
 * Banjo rides on Mario's controller; this adds Kazooie's Feathery Flap (press and hold A in the air).
 * Returns true when it consumed the frame.
 */
export function stepBanjo(m: MarioRuntime): boolean {
  if (!(m.action & ACT_FLAG_AIR)) {
    m.banjoFlap = 0;
    return false;
  }
  if (m.heldObj || m.ridingShell || (m.flags & MARIO_WING_CAP)) return false;
  const c = m.controller;
  if (!banjoFlapping(m)) {
    if (m.banjoFlap === 0 && FLAP_FROM.has(m.action) && (c.buttonPressed & A_BUTTON) && m.vel[1] < 12) {
      m.setAction(ACT_FREEFALL, BANJO_FLAP);
      m.banjoFlap = FLAP_FRAMES;
      m.vel[1] = 0;
      return stepFlap(m);
    }
    return false;
  }
  return stepFlap(m);
}

function stepFlap(m: MarioRuntime): boolean {
  const c = m.controller;
  if (c.buttonPressed & Z_TRIG) {
    m.banjoFlap = -1;
    m.setAction(ACT_GROUND_POUND);
    return false;
  }
  if (!(c.buttonDown & A_BUTTON) || --m.banjoFlap <= 0) {
    // Kazooie is spent until Banjo lands again.
    m.banjoFlap = -1;
    m.setAction(ACT_FREEFALL);
    return false;
  }
  if (m.actionTimer++ % FLAP_PERIOD === 0) m.vel[1] = FLAP_IMPULSE;
  else m.vel[1] = Math.max(FLAP_TERMINAL, m.vel[1] - FLAP_GRAVITY);

  const target = Math.min(FLAP_SPEED, m.intendedMag * FLAP_SPEED / 32);
  if (m.intendedMag > 0) {
    const turn = toS16(m.intendedYaw - m.faceAngle[1]);
    m.faceAngle[1] = toS16(m.faceAngle[1] + clamp(turn, -0x800, 0x800));
  }
  m.forwardVel = approachF32(m.forwardVel, target, 1, 1);
  m.vel[0] = sins(m.faceAngle[1]) * m.forwardVel;
  m.vel[2] = coss(m.faceAngle[1]) * m.forwardVel;

  const vy = m.vel[1];
  const hit = m.stepAir(AIR_STEP_CHECK_LEDGE_GRAB);
  // performAirStep applies SM64 gravity afterwards; Kazooie's flap replaces it.
  if (hit !== AIR_STEP_LANDED) m.vel[1] = vy;
  if (hit === AIR_STEP_LANDED) {
    m.landEvent(vy, m.forwardVel);
    m.setAction(ACT_FREEFALL_LAND);
  } else if (hit === AIR_STEP_GRABBED_LEDGE) m.setAction(ACT_LEDGE_GRAB);
  else if (hit === AIR_STEP_HIT_LAVA_WALL) m.setAction(ACT_LAVA_BOOST);
  return true;
}
