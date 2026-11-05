/**
 * Object-group actions: punching.  The animation-frame driven combo from
 * mario_actions_object.c is approximated with a frame timer, but the input
 * structure (first punch -> second punch -> kick) is preserved.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { marioSetForwardVel } from '../marioHelpers';
import {
  ACT_CROUCH_SLIDE,
  ACT_IDLE,
  ACT_JUMP_KICK,
  ACT_PICKING_UP,
  ACT_PLACING_DOWN,
  ACT_PUNCHING,
  ACT_THROWING,
  MARIO_KICKING,
  MARIO_PUNCHING,
  INPUT_ABOVE_SLIDE,
  INPUT_A_DOWN,
  INPUT_A_PRESSED,
  INPUT_B_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
} from '../state';
import { applySlopeAccel, applySlopeDecel } from './moving';
import { actPickingUp, actPlacingDown, actThrowing } from './holding';

const PUNCH_FORWARD_SPEEDS = [0, 1, 1, 2, 3, 5, 7, 10];

function punchDuration(arg: number): number {
  if (arg === 0) return 10;
  if (arg === 1) return 8;
  return 12;
}

/**
 * Simplified mario_update_punch_sequence: actionArg 0/1/2 selects punch/kick.
 * Returns true when the action ended.
 */
function updatePunchSequence(m: MarioRuntime): boolean {
  const elapsed = m.actionTimer++;
  const duration = punchDuration(m.actionArg);
  m.animLength = duration;

  marioSetForwardVel(m, PUNCH_FORWARD_SPEEDS[Math.min(elapsed, 7)]);

  if (m.input & INPUT_B_PRESSED && elapsed >= 3 && m.actionArg < 2) {
    m.actionArg++;
    m.actionTimer = 0;
    return false;
  }

  if (elapsed + 1 >= duration) {
    return m.setAction(ACT_IDLE, 0);
  }
  return false;
}

/** act_punching */
export function actPunching(m: MarioRuntime): boolean {
  if (m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)) {
    return m.checkCommonActionExits();
  }

  m.flags &= ~(MARIO_PUNCHING | MARIO_KICKING);
  if (m.actionArg < 2) m.flags |= MARIO_PUNCHING;
  else m.flags |= MARIO_KICKING;

  if (m.actionState === 0 && m.input & INPUT_A_DOWN) {
    return m.setAction(ACT_JUMP_KICK, 0);
  }

  m.actionState = 1;
  if (m.actionTimer >= 2 && m.actionArg < 2 && m.marioCheckObjectGrab()) return true;
  if (updatePunchSequence(m)) return true;

  m.stepGround();
  return false;
}

/** act_move_punching, driven by the moving group. */
export function actMovePunchingSimplified(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_DOWN) {
    return m.setAction(ACT_JUMP_KICK, 0);
  }

  m.flags &= ~(MARIO_PUNCHING | MARIO_KICKING);
  if (m.actionArg < 2) m.flags |= MARIO_PUNCHING;
  else m.flags |= MARIO_KICKING;

  marioSetForwardVel(m, PUNCH_FORWARD_SPEEDS[Math.min(m.actionTimer, 7)]);

  if (m.forwardVel >= 0) {
    applySlopeDecel(m, 0.5);
  } else {
    m.forwardVel += 8;
    if (m.forwardVel >= 0) m.forwardVel = 0;
    applySlopeAccel(m);
  }

  m.stepGround();

  if (++m.actionTimer >= 8) {
    return m.setAction(ACT_CROUCH_SLIDE, 0);
  }
  return false;
}

export const OBJECT_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_PUNCHING]: actPunching,
  [ACT_PICKING_UP]: actPickingUp,
  [ACT_PLACING_DOWN]: actPlacingDown,
  [ACT_THROWING]: actThrowing,
};
