/**
 * Action dispatch, mirroring execute_mario_action's group switch.
 *
 * Handlers return true when they changed Mario's action.  The runtime then
 * runs the loop again so the new action executes in the same 30 Hz frame,
 * exactly like the decompilation.
 */

import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_FREEFALL,
  ACT_GROUP_AIRBORNE,
  ACT_GROUP_AUTOMATIC,
  ACT_GROUP_CUTSCENE,
  ACT_GROUP_MOVING,
  ACT_GROUP_OBJECT,
  ACT_GROUP_STATIONARY,
  ACT_IDLE,
  ACT_STANDING_DEATH,
  ACT_WALKING,
} from '../state';
import { STATIONARY_ACTIONS } from './stationary';
import { MOVING_ACTIONS } from './moving';
import { AIRBORNE_ACTIONS } from './airborne';
import { AUTOMATIC_ACTIONS } from './automatic';
import { OBJECT_ACTIONS } from './object';
import { CUTSCENE_ACTIONS } from './cutscene';

const warned = new Set<number>();

function runAction(
  m: MarioRuntime,
  table: Record<number, (m: MarioRuntime) => boolean>,
  fallback: number,
): boolean {
  const handler = table[m.action];
  if (!handler) {
    if (!warned.has(m.action)) {
      warned.add(m.action);
      console.warn(
        `[jumpman] no handler for action 0x${m.action.toString(16)}; falling back to 0x${fallback.toString(16)}`,
      );
    }
    m.setAction(fallback, 0);
    return true;
  }
  return handler(m);
}

export function executeMarioAction(m: MarioRuntime): boolean {
  switch (m.action & 0x1c0) {
    case ACT_GROUP_STATIONARY:
      return runAction(m, STATIONARY_ACTIONS, ACT_IDLE);

    case ACT_GROUP_MOVING:
      return runAction(m, MOVING_ACTIONS, ACT_WALKING);

    case ACT_GROUP_AIRBORNE:
      return runAction(m, AIRBORNE_ACTIONS, ACT_FREEFALL);

    case ACT_GROUP_AUTOMATIC:
      return runAction(m, AUTOMATIC_ACTIONS, ACT_FREEFALL);

    case ACT_GROUP_OBJECT:
      return runAction(m, OBJECT_ACTIONS, ACT_IDLE);

    case ACT_GROUP_CUTSCENE:
      return runAction(m, CUTSCENE_ACTIONS, ACT_STANDING_DEATH);

    default:
      return false;
  }
}
