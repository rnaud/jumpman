/**
 * Object-group actions for picking up, placing and throwing objects, ported
 * from reference/sm64/src/game/mario_actions_object.c.
 */

import type { MarioRuntime } from '../MarioRuntime';
import {
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
