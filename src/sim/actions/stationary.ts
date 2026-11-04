/**
 * Stationary actions, ported from src/game/mario_actions_stationary.c.
 *
 * Sleeping, panting, first person, quicksand and damage reactions are omitted;
 * the movement-relevant actions are all here.
 */

import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_BACKFLIP,
  ACT_BUTT_SLIDE,
  ACT_BUTT_SLIDE_STOP,
  ACT_CROUCHING,
  ACT_FREEFALL,
  ACT_IDLE,
  ACT_JUMP,
  ACT_JUMP_LAND_STOP,
  ACT_DOUBLE_JUMP_LAND_STOP,
  ACT_SIDE_FLIP_LAND_STOP,
  ACT_TRIPLE_JUMP_LAND_STOP,
  ACT_BACKFLIP_LAND_STOP,
  ACT_LONG_JUMP_LAND_STOP,
  ACT_GROUND_POUND_LAND,
  ACT_BRAKING_STOP,
  ACT_START_CROUCHING,
  ACT_START_CRAWLING,
  ACT_STOP_CROUCHING,
  ACT_STOP_CRAWLING,
  ACT_SLIDE_KICK_SLIDE_STOP,
  ACT_PUNCHING,
  ACT_WALKING,
  ACT_BEGIN_SLIDING,
  ACT_FREEFALL_LAND_STOP,
  ACT_CRAWLING,
  INPUT_A_PRESSED,
  INPUT_ABOVE_SLIDE,
  INPUT_B_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
  INPUT_Z_DOWN,
} from '../state';
import { stationaryGroundStep } from '../marioStep';

/** check_common_idle_cancels (no objects or first-person mode). */
function checkCommonIdleCancels(m: MarioRuntime): boolean {
  if (m.floor && m.floor.normal[1] < 0.29237169) {
    return m.pushOffSteepFloor(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_JUMP, 0);
  }

  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  if (m.input & INPUT_NONZERO_ANALOG) {
    m.faceAngle[1] = m.intendedYaw;
    return m.setAction(ACT_WALKING, 0);
  }

  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(ACT_PUNCHING, 0);
  }

  if (m.input & INPUT_Z_DOWN) {
    return m.setAction(ACT_START_CROUCHING, 0);
  }

  return false;
}

export function actIdle(m: MarioRuntime): boolean {
  if (checkCommonIdleCancels(m)) return true;

  // Idle head-turn animation timing is purely cosmetic here.
  m.animLength = 90;
  stationaryGroundStep(m, m.world);
  return false;
}

export function actCrouching(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_BACKFLIP, 0);
  }

  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  if (!(m.input & INPUT_Z_DOWN)) {
    return m.setAction(ACT_STOP_CROUCHING, 0);
  }

  if (m.input & INPUT_NONZERO_ANALOG) {
    return m.setAction(ACT_START_CRAWLING, 0);
  }

  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(ACT_PUNCHING, 9);
  }

  stationaryGroundStep(m, m.world);
  m.animLength = 30;
  return false;
}

export function actStartCrouching(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_BACKFLIP, 0);
  }

  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= 6) {
    return m.setAction(ACT_CROUCHING, 0);
  }
  return false;
}

export function actStopCrouching(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_BACKFLIP, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= 6) {
    return m.setAction(ACT_IDLE, 0);
  }
  return false;
}

export function actStartCrawling(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= 8) {
    return m.setAction(ACT_CRAWLING, 0);
  }
  return false;
}

export function actStopCrawling(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BEGIN_SLIDING, 0);
  }

  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= 8) {
    return m.setAction(ACT_CROUCHING, 0);
  }
  return false;
}

/** stopping_step */
function stoppingStep(m: MarioRuntime, frames: number, action: number): boolean {
  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= frames) {
    return m.setAction(action, 0);
  }
  return false;
}

export function actBrakingStop(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(ACT_PUNCHING, 0);
  }

  if (
    m.input &
    (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)
  ) {
    return m.checkCommonActionExits();
  }

  return stoppingStep(m, 8, ACT_IDLE);
}

export function actButtSlideStop(m: MarioRuntime): boolean {
  if (m.input & (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)) {
    return m.checkCommonActionExits();
  }
  return stoppingStep(m, 8, ACT_IDLE);
}

export function actSlideKickSlideStop(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }
  return stoppingStep(m, 8, ACT_CROUCHING);
}

/** check_common_landing_cancels */
function checkCommonLandingCancels(m: MarioRuntime, action: number): boolean {
  if (m.input & INPUT_A_PRESSED) {
    if (!action) {
      return m.setJumpFromLanding();
    }
    return m.setJumpingAction(action, 0);
  }

  if (
    m.input &
    (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)
  ) {
    return m.checkCommonActionExits();
  }

  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(ACT_PUNCHING, 0);
  }

  return false;
}

/** landing_step */
function landingStep(m: MarioRuntime, frames: number, action: number): boolean {
  stationaryGroundStep(m, m.world);
  if (++m.actionTimer >= frames) {
    return m.setAction(action, 0);
  }
  return false;
}

export function actJumpLandStop(m: MarioRuntime): boolean {
  if (checkCommonLandingCancels(m, 0)) return true;
  return landingStep(m, 8, ACT_IDLE);
}

export function actDoubleJumpLandStop(m: MarioRuntime): boolean {
  if (checkCommonLandingCancels(m, 0)) return true;
  return landingStep(m, 8, ACT_IDLE);
}

export function actFreefallLandStop(m: MarioRuntime): boolean {
  if (checkCommonLandingCancels(m, 0)) return true;
  return landingStep(m, 8, ACT_IDLE);
}

export function actSideFlipLandStop(m: MarioRuntime): boolean {
  if (checkCommonLandingCancels(m, 0)) return true;
  if (landingStep(m, 8, ACT_IDLE)) return true;
  return false;
}

export function actTripleJumpLandStop(m: MarioRuntime): boolean {
  if (checkCommonLandingCancels(m, ACT_JUMP)) return true;
  return landingStep(m, 8, ACT_IDLE);
}

export function actBackflipLandStop(m: MarioRuntime): boolean {
  if (!(m.input & INPUT_Z_DOWN)) {
    m.input &= ~INPUT_A_PRESSED;
  }
  if (checkCommonLandingCancels(m, ACT_BACKFLIP)) return true;
  return landingStep(m, 8, ACT_IDLE);
}

export function actLongJumpLandStop(m: MarioRuntime): boolean {
  m.input &= ~INPUT_B_PRESSED;
  if (checkCommonLandingCancels(m, ACT_JUMP)) return true;
  return landingStep(m, 8, ACT_CROUCHING);
}

export function actGroundPoundLand(m: MarioRuntime): boolean {
  m.actionState = 1;

  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(ACT_FREEFALL, 0);
  }

  if (m.input & INPUT_ABOVE_SLIDE) {
    return m.setAction(ACT_BUTT_SLIDE, 0);
  }

  return landingStep(m, 14, ACT_BUTT_SLIDE_STOP);
}

/** Dispatch table for the stationary group. */
export const STATIONARY_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_IDLE]: actIdle,
  [ACT_CROUCHING]: actCrouching,
  [ACT_START_CROUCHING]: actStartCrouching,
  [ACT_STOP_CROUCHING]: actStopCrouching,
  [ACT_START_CRAWLING]: actStartCrawling,
  [ACT_STOP_CRAWLING]: actStopCrawling,
  [ACT_BRAKING_STOP]: actBrakingStop,
  [ACT_BUTT_SLIDE_STOP]: actButtSlideStop,
  [ACT_SLIDE_KICK_SLIDE_STOP]: actSlideKickSlideStop,
  [ACT_JUMP_LAND_STOP]: actJumpLandStop,
  [ACT_DOUBLE_JUMP_LAND_STOP]: actDoubleJumpLandStop,
  [ACT_FREEFALL_LAND_STOP]: actFreefallLandStop,
  [ACT_SIDE_FLIP_LAND_STOP]: actSideFlipLandStop,
  [ACT_TRIPLE_JUMP_LAND_STOP]: actTripleJumpLandStop,
  [ACT_BACKFLIP_LAND_STOP]: actBackflipLandStop,
  [ACT_LONG_JUMP_LAND_STOP]: actLongJumpLandStop,
  [ACT_GROUND_POUND_LAND]: actGroundPoundLand,
};
