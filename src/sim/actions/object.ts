/**
 * Object-group actions: punching. The punch combo is driven by Mario's
 * simulation animation frame, mirroring mario_update_punch_sequence from
 * mario_actions_object.c.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { stationaryGroundStep } from '../marioStep';
import { marioSetForwardVel } from '../marioHelpers';
import {
  ACT_CROUCHING,
  ACT_IDLE,
  ACT_JUMP_KICK,
  ACT_PICKING_UP_BOWSER, ACT_HOLDING_BOWSER, ACT_RELEASING_BOWSER,
  ACT_PICKING_UP, ACT_DIVE_PICKING_UP, ACT_HOLD_IDLE,
  ACT_PLACING_DOWN,
  ACT_PUNCHING,
  ACT_THROWING,
  MARIO_KICKING,
  MARIO_PUNCHING,
  MARIO_TRIPPING,
  INPUT_ABOVE_SLIDE,
  INPUT_A_DOWN,
  INPUT_A_PRESSED,
  INPUT_B_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
} from '../state';
import { actPickingUpBowser, actHoldingBowser, actReleasingBowser, actPickingUp, actPlacingDown, actThrowing } from './holding';

const PUNCH_FORWARD_SPEEDS = [0, 1, 1, 2, 3, 5, 7, 10];

// Source clips: first punch 5 frames, first fast punch 10, second punch 4,
// second fast punch 13, ground kick 26, breakdance 24.
const PUNCH_FRAMES = {
  first: 5,
  firstFast: 10,
  second: 4,
  secondFast: 13,
  kick: 26,
  breakdance: 24,
} as const;

function setPunchAnimation(m: MarioRuntime, frames: number): void {
  m.animLength = frames - 1;
}

/**
 * mario_update_punch_sequence, driven by the simulation's animation frame
 * rather than a wall-clock timer. actionArg 0/1 is the first punch, 2 the first
 * fast punch, 3/4 the second punch, 5 the second fast punch, 6 the ground kick
 * and 9 the breakdance trip.
 */
export function updatePunchSequence(
  m: MarioRuntime,
  endAction: number,
  crouchEndAction: number,
): boolean {
  switch (m.actionArg) {
    case 0:
    case 1:
      setPunchAnimation(m, PUNCH_FRAMES.first);
      if (m.animFrame >= 2) {
        if (m.marioCheckObjectGrab()) return true;
        m.flags |= MARIO_PUNCHING;
      }
      if (m.animFrame >= PUNCH_FRAMES.first - 1) {
        m.actionArg = 2;
        m.animFrame = -1;
      } else {
        m.actionArg = 1;
      }
      break;

    case 2:
      setPunchAnimation(m, PUNCH_FRAMES.firstFast);
      if (m.animFrame <= 0) m.flags |= MARIO_PUNCHING;
      if (m.input & INPUT_B_PRESSED) {
        m.actionArg = 3;
        m.animFrame = -1;
        break;
      }
      if (m.animFrame >= PUNCH_FRAMES.firstFast - 1) {
        return m.setAction(endAction, 0);
      }
      break;

    case 3:
    case 4:
      setPunchAnimation(m, PUNCH_FRAMES.second);
      if (m.animFrame > 0) m.flags |= MARIO_PUNCHING;
      if (m.animFrame >= PUNCH_FRAMES.second - 1) {
        m.actionArg = 5;
        m.animFrame = -1;
      } else {
        m.actionArg = 4;
      }
      break;

    case 5:
      setPunchAnimation(m, PUNCH_FRAMES.secondFast);
      if (m.animFrame <= 0) m.flags |= MARIO_PUNCHING;
      if (m.input & INPUT_B_PRESSED) {
        m.actionArg = 6;
        m.animFrame = -1;
        break;
      }
      if (m.animFrame >= PUNCH_FRAMES.secondFast - 1) {
        return m.setAction(endAction, 0);
      }
      break;

    case 6:
      setPunchAnimation(m, PUNCH_FRAMES.kick);
      if (m.animFrame >= 0 && m.animFrame < 8) m.flags |= MARIO_KICKING;
      if (m.animFrame >= PUNCH_FRAMES.kick - 1) {
        return m.setAction(endAction, 0);
      }
      break;

    case 9:
      setPunchAnimation(m, PUNCH_FRAMES.breakdance);
      if (m.animFrame >= 2 && m.animFrame < 8) m.flags |= MARIO_TRIPPING;
      if (m.animFrame >= PUNCH_FRAMES.breakdance - 1) {
        return m.setAction(crouchEndAction, 0);
      }
      break;
  }

  return false;
}

/** act_punching */
export function actPunching(m: MarioRuntime): boolean {
  if (m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)) {
    return m.checkCommonActionExits();
  }

  if (m.actionState === 0 && m.input & INPUT_A_DOWN) {
    return m.setAction(ACT_JUMP_KICK, 0);
  }
  m.actionState = 1;

  if (m.actionArg === 0 && m.actionTimer === 0) m.actionTimer = 7;
  marioSetForwardVel(m, PUNCH_FORWARD_SPEEDS[Math.min(7, m.actionTimer)]);
  if (m.actionTimer > 0) m.actionTimer--;

  if (updatePunchSequence(m, ACT_IDLE, ACT_CROUCHING)) return true;

  m.stepGround();
  return false;
}

export const OBJECT_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_PUNCHING]: actPunching,
  [ACT_PICKING_UP_BOWSER]: actPickingUpBowser,
  [ACT_HOLDING_BOWSER]: actHoldingBowser,
  [ACT_RELEASING_BOWSER]: actReleasingBowser,
  [ACT_DIVE_PICKING_UP]: m => {
    stationaryGroundStep(m,m.world);
    return ++m.actionTimer >= 37 && m.setAction(ACT_HOLD_IDLE);
  },
  [ACT_PICKING_UP]: actPickingUp,
  [ACT_PLACING_DOWN]: actPlacingDown,
  [ACT_THROWING]: actThrowing,
};
