import type { MarioRuntime } from './MarioRuntime';
import { approachF32, clamp, coss, sins, toS16 } from './math';
import { marioSetForwardVel } from './marioHelpers';
import {
  ACT_FREEFALL, ACT_IDLE, ACT_PUNCHING, ACT_WALKING, ACT_LEDGE_GRAB,
  AIR_STEP_CHECK_LEDGE_GRAB, AIR_STEP_GRABBED_LEDGE, AIR_STEP_HIT_LAVA_WALL,
  AIR_STEP_LANDED, ACT_LAVA_BOOST, A_BUTTON, B_BUTTON, Z_TRIG, INPUT_OFF_FLOOR, GROUND_STEP_HIT_WALL,
  GROUND_STEP_LEFT_GROUND, MARIO_PUNCHING,
} from './state';

// Reuse walking and attacking IDs so course actors see Link through their usual interactions.
export const LINK_BACKFLIP = 0x50;
export const LINK_HOP_LEFT = 0x51;
export const LINK_HOP_RIGHT = 0x52;
const SLASH = 0x53;
const FINISHER = 0x54;
export const linkFinishing = (m: MarioRuntime) => m.action === ACT_PUNCHING && m.actionArg === FINISHER;
const ROLL = 1;
export function swordReach(m: MarioRuntime): number {
  return m.action === ACT_PUNCHING && (m.actionArg === SLASH || m.actionArg === FINISHER) && (m.flags & MARIO_PUNCHING) ? 82 : 0;
}
const GUARD = 0x4c;
export const linkGuarding = (m: MarioRuntime) => m.action === ACT_IDLE && m.actionArg === GUARD;
export const linkRolling = (m: MarioRuntime) => m.action === ACT_WALKING && m.actionArg === ROLL;

/** OoT free movement adapted to Jumpman's units and 30 Hz collision steps. */
export function stepLink(m: MarioRuntime): boolean {
  if (m.heldObj || m.ridingShell) return false;
  if (m.action === ACT_FREEFALL) {
    m.actionTimer++;
    const vy = m.vel[1];
    const hit = m.stepAir(AIR_STEP_CHECK_LEDGE_GRAB);
    if (hit === AIR_STEP_LANDED) {
      m.landEvent(vy, m.forwardVel);
      m.setAction(ACT_IDLE);
    } else if (hit === AIR_STEP_GRABBED_LEDGE) m.setAction(ACT_LEDGE_GRAB);
    else if (hit === AIR_STEP_HIT_LAVA_WALL) m.setAction(ACT_LAVA_BOOST);
    return true;
  }
  if (m.action !== ACT_IDLE && m.action !== ACT_WALKING && m.action !== ACT_PUNCHING) return false;

  if (m.input & INPUT_OFF_FLOOR) {
    m.setAction(ACT_FREEFALL);
    return true;
  }
  // Player_CalcSpeedAndYawFromControlStick's curved mode, scaled to course units.
  const magnitude = m.movementMode === 'side' ? Math.abs(m.controller.stickX) : m.controller.stickMag;
  const curve = 1 - coss((magnitude - 20) * 450);
  const target = magnitude < 20 ? 0 : Math.min(32, (curve * curve * 30 + 7) * 0.56);

  if (!linkRolling(m) && m.action !== ACT_PUNCHING) {
    if (m.controller.focusYaw !== undefined) m.faceAngle[1] = m.controller.focusYaw;
    else if (target > 0 && !linkGuarding(m)) {
      const turn = toS16(m.intendedYaw - m.faceAngle[1]);
      m.faceAngle[1] = m.forwardVel < 1 ? m.intendedYaw : toS16(m.faceAngle[1] + clamp(turn, -0x1600, 0x1600));
    }
    if (m.controller.buttonPressed & B_BUTTON) m.setAction(ACT_PUNCHING, SLASH);
    else if (m.controller.focusYaw !== undefined && target > 0 && (m.controller.buttonPressed & A_BUTTON)
        && Math.abs(toS16(m.intendedYaw - m.faceAngle[1])) > 0x2000) {
      const delta = toS16(m.intendedYaw - m.faceAngle[1]);
      const backwards = Math.abs(delta) > 0x6000;
      const dodgeYaw = m.faceAngle[1] + (backwards ? 0x8000 : Math.sign(delta) * 0x4000);
      m.setAction(ACT_FREEFALL, backwards ? LINK_BACKFLIP : delta > 0 ? LINK_HOP_LEFT : LINK_HOP_RIGHT);
      m.forwardVel = 27;
      m.vel = [sins(dodgeYaw) * 27, backwards ? 38 : 28, coss(dodgeYaw) * 27];
      return true;
    } else if (m.controller.buttonDown & Z_TRIG) {
      if (!linkGuarding(m)) m.setAction(ACT_IDLE, GUARD);
    } else if (target > 0 && m.controller.buttonPressed & A_BUTTON) m.setAction(ACT_WALKING, ROLL);
    else {
      const action = target > 0 || m.forwardVel > 2 ? ACT_WALKING : ACT_IDLE;
      if (m.action !== action || linkGuarding(m)) m.setAction(action);
    }
  }

  let speed = linkGuarding(m) ? 0 : target;
  if (m.action === ACT_PUNCHING) {
    speed = 0;
    const finishing = linkFinishing(m);
    if (!finishing && m.actionTimer >= 4 && (m.controller.buttonPressed & B_BUTTON)) m.actionState = 1;
    if (m.actionTimer >= 2 && m.actionTimer < (finishing ? 6 : 5)) m.flags |= MARIO_PUNCHING;
    m.actionTimer++;
    if (!finishing && m.actionState === 1 && m.actionTimer >= 9) m.setAction(ACT_PUNCHING, FINISHER);
    else if (m.actionTimer >= (finishing ? 25 : 13)) m.setAction(ACT_IDLE);
  } else if (linkRolling(m)) {
    // Player_Action_Roll uses 1.5x stick speed and exits at animation frame 20.
    speed = Math.max(12, target * 1.5);
    if (m.actionTimer === 6) m.invincTimer = Math.max(m.invincTimer, 10);
    if (++m.actionTimer >= 16) m.setAction(ACT_WALKING);
  }
  marioSetForwardVel(m, approachF32(m.forwardVel, speed, 4, 6));
  if (m.controller.focusYaw !== undefined && m.action === ACT_WALKING && !linkRolling(m)) {
    m.vel[0] = sins(m.intendedYaw) * m.forwardVel;
    m.vel[2] = coss(m.intendedYaw) * m.forwardVel;
  }
  const hit = m.stepGround();
  if (hit === GROUND_STEP_LEFT_GROUND) {
    m.setAction(ACT_FREEFALL);
    m.vel[1] = m.forwardVel >= 12 ? 38 : 0;
  } else if (hit === GROUND_STEP_HIT_WALL && linkRolling(m)) {
    m.setAction(ACT_IDLE);
    marioSetForwardVel(m, 0);
  }
  return true;
}
