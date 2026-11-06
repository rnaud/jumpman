/**
 * Object-group actions for picking up, placing and throwing objects, ported
 * from reference/sm64/src/game/mario_actions_object.c.
 */

import { stationaryGroundStep } from '../marioStep';
import { approachS32, clamp, toS16 } from '../math';
import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_HOLDING_BOWSER, ACT_RELEASING_BOWSER, INPUT_B_PRESSED,
  ACT_FREEFALL,
  ACT_HOLD_IDLE,
  ACT_IDLE,
  ACT_PLACING_DOWN,
  INPUT_OFF_FLOOR,
} from '../state';
import { INT_SUBTYPE_HOLDABLE_NPC } from '../object';

export function actPickingUp(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    m.dropHeldObject();
    return m.setAction(ACT_FREEFALL, 0);
  }

  m.actionTimer++;
  m.animLength = 20;
  if (m.actionState === 0 && m.actionTimer >= 5) {
    m.grabUsedObject();
    m.actionState = 1;
  }

  if (m.actionState === 1 && m.actionTimer >= 20) {
    return m.setAction(ACT_HOLD_IDLE, 0);
  }

  m.stepGround();
  return false;
}

export function actPlacingDown(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    m.dropHeldObject();
    return m.setAction(ACT_FREEFALL, 0);
  }

  m.actionTimer++;
  m.animLength = 20;
  if (m.actionTimer === 8) m.dropHeldObject();
  m.stepGround();

  if (m.actionTimer >= 20) return m.setAction(ACT_IDLE, 0);
  return false;
}

export function actThrowing(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    m.dropHeldObject();
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.heldObj && m.heldObj.interactSubtype & INT_SUBTYPE_HOLDABLE_NPC) {
    return m.setAction(ACT_PLACING_DOWN, 0);
  }

  m.actionTimer++;
  m.animLength = 12;
  if (m.actionTimer === 7) m.throwHeldObject();
  m.stepGround();

  if (m.actionTimer >= 12) return m.setAction(ACT_IDLE, 0);
  return false;
}

export function actPickingUpBowser(m: MarioRuntime): boolean {
  if (!m.actionTimer) {
    m.angleVel[1] = 0;
    m.grabUsedObject();
  }
  stationaryGroundStep(m, m.world);
  return ++m.actionTimer >= 20 && m.setAction(ACT_HOLDING_BOWSER, 0);
}

export function actHoldingBowser(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_RELEASING_BOWSER, 0);
  if (!m.angleVel[1]) {
    if (m.actionTimer++ > 120) return m.setAction(ACT_RELEASING_BOWSER, 1);
  } else m.actionTimer = 0;
  if (m.intendedMag > 20) {
    if (m.actionArg) {
      const spin = clamp(Math.trunc(toS16(m.intendedYaw - m.twirlYaw) / 0x80), -0x80, 0x80);
      m.angleVel[1] = clamp(m.angleVel[1] + spin, -0x1000, 0x1000);
    }
    m.actionArg = 1;
    m.twirlYaw = m.intendedYaw;
  } else {
    m.actionArg = 0;
    m.angleVel[1] = approachS32(m.angleVel[1], 0, 64, 64);
  }
  m.faceAngle[1] = toS16(m.faceAngle[1] + m.angleVel[1]);
  stationaryGroundStep(m, m.world);
  return false;
}

export function actReleasingBowser(m: MarioRuntime): boolean {
  if (++m.actionTimer === 1 && m.heldObj) {
    const boss = m.heldObj;
    const pos = [...boss.pos] as typeof boss.pos;
    boss.vars.spin = m.angleVel[1];
    if (m.actionArg) m.dropHeldObject();
    else m.throwHeldObject();
    boss.pos = pos;
    m.angleVel[1] = 0;
  }
  stationaryGroundStep(m, m.world);
  return m.actionTimer >= 40 && m.setAction(ACT_IDLE);
}
