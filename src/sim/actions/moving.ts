/**
 * Moving (ground) actions, ported from src/game/mario_actions_moving.c.
 *
 * This file contains the acceleration model that gives Mario his feel:
 * analog target speed, slope acceleration, braking, skidding, butt slides,
 * stomach slides, crouch slides, dives and landing cancels.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { approachF32, approachS32, atan2s, toS16 } from '../math';
import {
  marioFloorIsSlope,
  marioFloorIsSlippery,
  marioFacingDownhill,
  marioSetForwardVel,
} from '../marioHelpers';
import {
  ACT_SOFT_BACKWARD_GROUND_KB, ACT_SOFT_FORWARD_GROUND_KB, ACT_BACKWARD_GROUND_KB, ACT_FORWARD_GROUND_KB, ACT_FORWARD_AIR_KB, ACT_BACKWARD_AIR_KB,
  ACT_HARD_BACKWARD_GROUND_KB, ACT_HARD_FORWARD_GROUND_KB, ACT_HARD_FORWARD_AIR_KB, ACT_HARD_BACKWARD_AIR_KB,
  ACT_BACKFLIP_LAND,
  ACT_BRAKING,
  ACT_BRAKING_STOP,
  ACT_BUTT_SLIDE,
  ACT_BUTT_SLIDE_AIR,
  ACT_BUTT_SLIDE_STOP,
  ACT_CRAWLING,
  ACT_CROUCHING,
  ACT_CROUCH_SLIDE,
  ACT_DECELERATING,
  ACT_DIVE,
  ACT_DIVE_SLIDE,
  ACT_DOUBLE_JUMP,
  ACT_DOUBLE_JUMP_LAND,
  ACT_DOUBLE_JUMP_LAND_STOP,
  ACT_FREEFALL,
  ACT_FREEFALL_LAND,
  ACT_FREEFALL_LAND_STOP,
  ACT_IDLE,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_JUMP_LAND,
  ACT_JUMP_LAND_STOP,
  ACT_LONG_JUMP,
  ACT_LONG_JUMP_LAND,
  ACT_LONG_JUMP_LAND_STOP,
  ACT_MOVE_PUNCHING,
  ACT_SIDE_FLIP,
  ACT_SIDE_FLIP_LAND,
  ACT_SIDE_FLIP_LAND_STOP,
  ACT_SLIDE_KICK,
  ACT_SLIDE_KICK_SLIDE,
  ACT_SLIDE_KICK_SLIDE_STOP,
  ACT_STOMACH_SLIDE,
  ACT_TRIPLE_JUMP,
  ACT_TRIPLE_JUMP_LAND,
  ACT_TRIPLE_JUMP_LAND_STOP,
  ACT_TURNING_AROUND,
  ACT_FINISH_TURNING_AROUND,
  ACT_WALKING,
  ACT_RIDING_SHELL_GROUND, ACT_RIDING_SHELL_JUMP, ACT_RIDING_SHELL_FALL,
  ACT_HOLD_DECELERATING,
  ACT_HOLD_FREEFALL,
  ACT_HOLD_FREEFALL_LAND,
  ACT_HOLD_IDLE,
  ACT_HOLD_JUMP,
  ACT_HOLD_JUMP_LAND,
  ACT_HOLD_WALKING,
  ACT_PLACING_DOWN,
  ACT_THROWING,
  ACT_BEGIN_SLIDING, ACT_HOLD_BEGIN_SLIDING, ACT_HOLD_BUTT_SLIDE, ACT_HOLD_BUTT_SLIDE_AIR, ACT_HOLD_BUTT_SLIDE_STOP, ACT_HOLD_STOMACH_SLIDE, ACT_DIVE_PICKING_UP,
  GROUND_STEP_HIT_WALL,
  GROUND_STEP_LEFT_GROUND,
  GROUND_STEP_NONE,
  INPUT_ABOVE_SLIDE,
  INPUT_A_DOWN,
  INPUT_A_PRESSED,
  INPUT_B_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
  INPUT_UNKNOWN_5,
  INPUT_Z_DOWN,
  INPUT_Z_PRESSED,
} from '../state';
import { INT_SUBTYPE_HOLDABLE_NPC } from '../object';
import {
  SURFACE_CLASS_NOT_SLIPPERY,
  SURFACE_CLASS_SLIPPERY,
  SURFACE_CLASS_VERY_SLIPPERY,
  SURFACE_SLOW,
} from '../surfaces';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** apply_slope_accel */
export function applySlopeAccel(m: MarioRuntime): void {
  const floor = m.floor;
  if (!floor) return;

  const steepness = Math.sqrt(floor.normal[0] * floor.normal[0] + floor.normal[2] * floor.normal[2]);
  const floorDYaw = toS16(m.floorAngle - m.faceAngle[1]);

  if (marioFloorIsSlope(m)) {
    let slopeAccel: number;
    switch (m.floorClass()) {
      case SURFACE_CLASS_VERY_SLIPPERY:
        slopeAccel = 5.3;
        break;
      case SURFACE_CLASS_SLIPPERY:
        slopeAccel = 2.7;
        break;
      case SURFACE_CLASS_NOT_SLIPPERY:
        slopeAccel = 0;
        break;
      default:
        slopeAccel = 1.7;
        break;
    }

    if (floorDYaw > -0x4000 && floorDYaw < 0x4000) {
      m.forwardVel += slopeAccel * steepness;
    } else {
      m.forwardVel -= slopeAccel * steepness;
    }
  }

  m.slideYaw = m.faceAngle[1];
  m.slideVelX = m.forwardVel * Math.sin(toS16(m.faceAngle[1]) / 32768 * Math.PI);
  m.slideVelZ = m.forwardVel * Math.cos(toS16(m.faceAngle[1]) / 32768 * Math.PI);
  m.vel[0] = m.slideVelX;
  m.vel[1] = 0;
  m.vel[2] = m.slideVelZ;
}

/** apply_landing_accel */
export function applyLandingAccel(m: MarioRuntime, frictionFactor: number): boolean {
  applySlopeAccel(m);

  let stopped = false;
  if (!marioFloorIsSlope(m)) {
    m.forwardVel *= frictionFactor;
    if (m.forwardVel * m.forwardVel < 1) {
      marioSetForwardVel(m, 0);
      stopped = true;
    }
  }
  return stopped;
}

/** apply_slope_decel */
export function applySlopeDecel(m: MarioRuntime, decelCoef: number): boolean {
  let decel: number;
  switch (m.floorClass()) {
    case SURFACE_CLASS_VERY_SLIPPERY:
      decel = decelCoef * 0.2;
      break;
    case SURFACE_CLASS_SLIPPERY:
      decel = decelCoef * 0.7;
      break;
    case SURFACE_CLASS_NOT_SLIPPERY:
      decel = decelCoef * 3;
      break;
    default:
      decel = decelCoef * 2;
      break;
  }

  let stopped = false;
  m.forwardVel = approachF32(m.forwardVel, 0, decel, decel);
  if (m.forwardVel === 0) stopped = true;

  applySlopeAccel(m);
  return stopped;
}

/** update_decelerating_speed */
function updateDeceleratingSpeed(m: MarioRuntime): boolean {
  let stopped = false;
  m.forwardVel = approachF32(m.forwardVel, 0, 1, 1);
  if (m.forwardVel === 0) stopped = true;
  marioSetForwardVel(m, m.forwardVel);
  return stopped;
}

/** update_walking_speed */
export function updateWalkingSpeed(m: MarioRuntime): void {
  const maxTargetSpeed = m.floor && m.floor.type === SURFACE_SLOW ? 24 : 32;
  const targetSpeed = Math.min(m.intendedMag, maxTargetSpeed);

  if (m.forwardVel <= 0) {
    m.forwardVel += 1.1;
  } else if (m.forwardVel <= targetSpeed) {
    m.forwardVel += 1.1 - m.forwardVel / 43;
  } else if (m.floor && m.floor.normal[1] >= 0.95) {
    m.forwardVel -= 1;
  }

  if (m.forwardVel > 48) m.forwardVel = 48;

  m.faceAngle[1] = toS16(
    m.intendedYaw - approachS32(toS16(m.intendedYaw - m.faceAngle[1]), 0, 0x800, 0x800),
  );
  applySlopeAccel(m);
}

/** should_begin_sliding */
function shouldBeginSliding(m: MarioRuntime): boolean {
  if (m.input & INPUT_ABOVE_SLIDE) {
    const movingBackward = m.forwardVel <= -1;
    if (m.slideTerrain || movingBackward || marioFacingDownhill(m, false)) {
      return true;
    }
  }
  return false;
}

/** analog_stick_held_back */
function analogStickHeldBack(m: MarioRuntime): boolean {
  const intendedDYaw = toS16(m.intendedYaw - m.faceAngle[1]);
  return intendedDYaw < -0x471c || intendedDYaw > 0x471c;
}

/** check_ground_dive_or_punch */
function checkGroundDiveOrPunch(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) {
    if (m.forwardVel >= 29 && m.controller.stickMag > 48) {
      m.vel[1] = 20;
      return m.setAction(ACT_DIVE, 1);
    }
    return m.setAction(ACT_MOVE_PUNCHING, 0);
  }
  return false;
}

/** begin_braking_action */
function beginBrakingAction(m: MarioRuntime): boolean {
  if (m.forwardVel >= 16 && m.floor && m.floor.normal[1] >= 0.17364818) {
    return m.setAction(ACT_BRAKING, 0);
  }
  return m.setAction(ACT_DECELERATING, 0);
}

/** begin_walking_action */
function beginWalkingAction(m: MarioRuntime, forwardVel: number, action: number): boolean {
  m.faceAngle[1] = m.intendedYaw;
  marioSetForwardVel(m, forwardVel);
  return m.setAction(action, 0);
}

/** slide_bonk */
function slideBonk(m: MarioRuntime, slowAction: number): void {
  if (m.forwardVel > 16) {
    m.bonkReflection(true);
    // ACT_GROUND_BONK is a knockback action; decelerating is the closest
    // non-damage equivalent in this port.
    m.setAction(ACT_DECELERATING, 0);
  } else {
    marioSetForwardVel(m, 0);
    m.setAction(slowAction, 0);
  }
}

/** set_triple_jump_action */
function setTripleJumpAction(m: MarioRuntime): boolean {
  if (m.forwardVel > 20) {
    return m.setAction(ACT_TRIPLE_JUMP, 0);
  }
  return m.setAction(ACT_JUMP, 0);
}

function sin(angle: number): number {
  return Math.sin(toS16(angle) / 32768 * Math.PI);
}
function cos(angle: number): number {
  return Math.cos(toS16(angle) / 32768 * Math.PI);
}

// ---------------------------------------------------------------------------
// Sliding
// ---------------------------------------------------------------------------

/** update_sliding_angle */
function updateSlidingAngle(m: MarioRuntime, accel: number, lossFactor: number): void {
  const floor = m.floor;
  if (!floor) return;

  const slopeAngle = atan2s(floor.normal[2], floor.normal[0]);
  const steepness = Math.sqrt(floor.normal[0] * floor.normal[0] + floor.normal[2] * floor.normal[2]);

  m.slideVelX += accel * steepness * sin(slopeAngle);
  m.slideVelZ += accel * steepness * cos(slopeAngle);
  m.slideVelX *= lossFactor;
  m.slideVelZ *= lossFactor;

  m.slideYaw = atan2s(m.slideVelZ, m.slideVelX);

  const facingDYaw = toS16(m.faceAngle[1] - m.slideYaw);
  let newFacingDYaw = facingDYaw;

  if (newFacingDYaw > 0 && newFacingDYaw <= 0x4000) {
    newFacingDYaw -= 0x200;
    if (newFacingDYaw < 0) newFacingDYaw = 0;
  } else if (newFacingDYaw > -0x4000 && newFacingDYaw < 0) {
    newFacingDYaw += 0x200;
    if (newFacingDYaw > 0) newFacingDYaw = 0;
  } else if (newFacingDYaw > 0x4000 && newFacingDYaw < 0x8000) {
    newFacingDYaw += 0x200;
    if (newFacingDYaw > 0x8000) newFacingDYaw = 0x8000;
  } else if (newFacingDYaw > -0x8000 && newFacingDYaw < -0x4000) {
    newFacingDYaw -= 0x200;
    if (newFacingDYaw < -0x8000) newFacingDYaw = -0x8000;
  }

  m.faceAngle[1] = toS16(m.slideYaw + newFacingDYaw);

  m.vel[0] = m.slideVelX;
  m.vel[1] = 0;
  m.vel[2] = m.slideVelZ;

  m.forwardVel = Math.sqrt(m.slideVelX * m.slideVelX + m.slideVelZ * m.slideVelZ);
  if (m.forwardVel > 100) {
    m.slideVelX = (m.slideVelX * 100) / m.forwardVel;
    m.slideVelZ = (m.slideVelZ * 100) / m.forwardVel;
  }

  if (newFacingDYaw < -0x4000 || newFacingDYaw > 0x4000) {
    m.forwardVel *= -1;
  }
}

/** update_sliding */
function updateSliding(m: MarioRuntime, stopSpeed: number): boolean {
  let accel: number;
  let lossFactor: number;

  const intendedDYaw = toS16(m.intendedYaw - m.slideYaw);
  let forward = cos(intendedDYaw);
  const sideward = sin(intendedDYaw);

  if (forward < 0 && m.forwardVel >= 0) {
    forward *= 0.5 + (0.5 * m.forwardVel) / 100;
  }

  switch (m.floorClass()) {
    case SURFACE_CLASS_VERY_SLIPPERY:
      accel = 10;
      lossFactor = (m.intendedMag / 32) * forward * 0.02 + 0.98;
      break;
    case SURFACE_CLASS_SLIPPERY:
      accel = 8;
      lossFactor = (m.intendedMag / 32) * forward * 0.02 + 0.96;
      break;
    case SURFACE_CLASS_NOT_SLIPPERY:
      accel = 5;
      lossFactor = (m.intendedMag / 32) * forward * 0.02 + 0.92;
      break;
    default:
      accel = 7;
      lossFactor = (m.intendedMag / 32) * forward * 0.02 + 0.92;
      break;
  }

  const oldSpeed = Math.sqrt(m.slideVelX * m.slideVelX + m.slideVelZ * m.slideVelZ);

  m.slideVelX += m.slideVelZ * (m.intendedMag / 32) * sideward * 0.05;
  m.slideVelZ -= m.slideVelX * (m.intendedMag / 32) * sideward * 0.05;

  const newSpeed = Math.sqrt(m.slideVelX * m.slideVelX + m.slideVelZ * m.slideVelZ);
  if (oldSpeed > 0 && newSpeed > 0) {
    m.slideVelX = (m.slideVelX * oldSpeed) / newSpeed;
    m.slideVelZ = (m.slideVelZ * oldSpeed) / newSpeed;
  }

  updateSlidingAngle(m, accel, lossFactor);

  let stopped = false;
  if (!marioFloorIsSlope(m) && m.forwardVel * m.forwardVel < stopSpeed * stopSpeed) {
    marioSetForwardVel(m, 0);
    stopped = true;
  }
  return stopped;
}

/** common_slide_action */
function commonSlideAction(m: MarioRuntime, endAction: number, airAction: number): void {
  const step = m.stepGround();

  switch (step) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(airAction, 0);
      break;

    case GROUND_STEP_NONE:
      if (m.floor) m.pos[1] = m.floorHeight;
      break;

    case GROUND_STEP_HIT_WALL:
      if (!marioFloorIsSlippery(m)) {
        slideBonk(m, endAction);
      } else if (m.wall) {
        const wallAngle = atan2s(m.wall.normal[2], m.wall.normal[0]);
        let slideSpeed = Math.sqrt(m.slideVelX * m.slideVelX + m.slideVelZ * m.slideVelZ);
        slideSpeed *= 0.9;
        if (slideSpeed < 4) slideSpeed = 4;

        m.slideYaw = toS16(wallAngle - toS16(m.slideYaw - wallAngle) + 0x8000);
        m.vel[0] = m.slideVelX = slideSpeed * sin(m.slideYaw);
        m.vel[2] = m.slideVelZ = slideSpeed * cos(m.slideYaw);
      }
      if (m.floor) m.pos[1] = m.floorHeight;
      break;
  }
}

/** common_slide_action_with_jump */
function commonSlideActionWithJump(
  m: MarioRuntime,
  stopAction: number,
  jumpAction: number,
  airAction: number,
): boolean {
  if (m.actionTimer === 5) {
    if (m.input & INPUT_A_PRESSED) {
      return m.setJumpingAction(jumpAction, 0);
    }
  } else {
    m.actionTimer++;
  }

  if (updateSliding(m, 4)) {
    return m.setAction(stopAction, 0);
  }

  commonSlideAction(m, stopAction, airAction);
  return false;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

export function actWalking(m: MarioRuntime): boolean {
  if (shouldBeginSliding(m)) return m.setAction(ACT_BEGIN_SLIDING, 0);

  if (m.input & INPUT_A_PRESSED) return m.setJumpFromLanding();
  if (checkGroundDiveOrPunch(m)) return true;
  if (m.input & INPUT_UNKNOWN_5) return beginBrakingAction(m);
  if (analogStickHeldBack(m) && m.forwardVel >= 16) {
    return m.setAction(ACT_TURNING_AROUND, 0);
  }
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_CROUCH_SLIDE, 0);

  m.actionState = 0;
  updateWalkingSpeed(m);

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;

    case GROUND_STEP_NONE:
      if (m.intendedMag - m.forwardVel > 16) m.particleFlags |= 1;
      break;

    case GROUND_STEP_HIT_WALL:
      if (m.forwardVel > 6) marioSetForwardVel(m, 6);
      m.actionTimer = 0;
      break;
  }

  return false;
}

export function actMovePunching(m: MarioRuntime): boolean {
  if (shouldBeginSliding(m)) return m.setAction(ACT_BEGIN_SLIDING, 0);
  if (m.actionState === 0 && (m.input & INPUT_A_DOWN)) {
    return m.setAction(ACT_JUMP_KICK, 0);
  }

  m.actionState = 1;

  if (m.forwardVel >= 0) {
    applySlopeDecel(m, 0.5);
  } else {
    m.forwardVel += 8;
    if (m.forwardVel >= 0) m.forwardVel = 0;
    applySlopeAccel(m);
  }

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;
    case GROUND_STEP_NONE:
      m.particleFlags |= 1;
      break;
  }

  if (++m.actionTimer >= 8) {
    return m.setAction(ACT_WALKING, 0);
  }

  return false;
}

export function actTurningAround(m: MarioRuntime): boolean {
  if (m.input & INPUT_ABOVE_SLIDE) return m.setAction(ACT_BEGIN_SLIDING, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_SIDE_FLIP, 0);
  if (m.input & INPUT_UNKNOWN_5) return m.setAction(ACT_BRAKING, 0);
  if (!analogStickHeldBack(m)) return m.setAction(ACT_WALKING, 0);

  if (applySlopeDecel(m, 2)) {
    return beginWalkingAction(m, 8, ACT_FINISH_TURNING_AROUND);
  }

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;
    case GROUND_STEP_NONE:
      m.particleFlags |= 1;
      break;
  }

  if (m.forwardVel >= 18) {
    m.animLength = 12;
  } else {
    m.animLength = 18;
    if (++m.actionTimer > 18) {
      if (m.forwardVel > 0) {
        beginWalkingAction(m, -m.forwardVel, ACT_WALKING);
      } else {
        beginWalkingAction(m, 8, ACT_WALKING);
      }
    }
  }

  return false;
}

export function actFinishTurningAround(m: MarioRuntime): boolean {
  if (m.input & INPUT_ABOVE_SLIDE) return m.setAction(ACT_BEGIN_SLIDING, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_SIDE_FLIP, 0);

  updateWalkingSpeed(m);

  if (m.stepGround() === GROUND_STEP_LEFT_GROUND) {
    m.setAction(ACT_FREEFALL, 0);
  }

  if (++m.actionTimer >= 12) {
    return m.setAction(ACT_WALKING, 0);
  }
  return false;
}

export function actBraking(m: MarioRuntime): boolean {
  if (
    m.input &
    (INPUT_NONZERO_ANALOG | INPUT_A_PRESSED | INPUT_OFF_FLOOR | INPUT_ABOVE_SLIDE)
  ) {
    return m.checkCommonActionExits();
  }

  if (applySlopeDecel(m, 2)) {
    return m.setAction(ACT_BRAKING_STOP, 0);
  }

  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(ACT_MOVE_PUNCHING, 0);
  }

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;
    case GROUND_STEP_NONE:
      m.particleFlags |= 1;
      break;
    case GROUND_STEP_HIT_WALL:
      slideBonk(m, ACT_BRAKING_STOP);
      break;
  }

  m.animLength = 20;
  return false;
}

export function actDecelerating(m: MarioRuntime): boolean {
  const slopeClass = m.floorClass();

  if (shouldBeginSliding(m)) return m.setAction(ACT_BEGIN_SLIDING, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpFromLanding();
  if (checkGroundDiveOrPunch(m)) return true;
  if (m.input & INPUT_NONZERO_ANALOG) return m.setAction(ACT_WALKING, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_CROUCH_SLIDE, 0);

  if (updateDeceleratingSpeed(m)) {
    return m.setAction(ACT_IDLE, 0);
  }

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;
    case GROUND_STEP_HIT_WALL:
      if (slopeClass === SURFACE_CLASS_VERY_SLIPPERY) {
        m.bonkReflection(true);
      } else {
        marioSetForwardVel(m, 0);
      }
      break;
  }

  if (slopeClass === SURFACE_CLASS_VERY_SLIPPERY) {
    m.particleFlags |= 1;
  }
  m.animLength = 30;
  return false;
}

export function actCrawling(m: MarioRuntime): boolean {
  if (shouldBeginSliding(m)) return m.setAction(ACT_BEGIN_SLIDING, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_JUMP, 0);
  if (checkGroundDiveOrPunch(m)) return true;
  if (m.input & INPUT_UNKNOWN_5) return m.setAction(0x0c008224, 0); // ACT_STOP_CRAWLING
  if (!(m.input & INPUT_Z_DOWN)) return m.setAction(0x0c008224, 0);

  m.intendedMag *= 0.1;
  updateWalkingSpeed(m);

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 0);
      break;
    case GROUND_STEP_HIT_WALL:
      if (m.forwardVel > 10) marioSetForwardVel(m, 10);
      break;
  }

  m.animLength = 20;
  return false;
}

export function actCrouchSlide(m: MarioRuntime): boolean {
  if (m.input & INPUT_ABOVE_SLIDE) return m.setAction(ACT_BUTT_SLIDE, 0);

  if (m.actionTimer < 30) {
    m.actionTimer++;
    if (m.input & INPUT_A_PRESSED && m.forwardVel > 10) {
      return m.setJumpingAction(ACT_LONG_JUMP, 0);
    }
  }

  if (m.input & INPUT_B_PRESSED) {
    if (m.forwardVel >= 10) {
      return m.setAction(ACT_SLIDE_KICK, 0);
    }
    return m.setAction(ACT_MOVE_PUNCHING, 0x0009);
  }

  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_JUMP, 0);
  }

  return commonSlideActionWithJump(m, ACT_CROUCHING, ACT_JUMP, ACT_FREEFALL);
}

export function actButtSlide(m: MarioRuntime): boolean {
  return commonSlideActionWithJump(m, ACT_BUTT_SLIDE_STOP, ACT_JUMP, ACT_BUTT_SLIDE_AIR);
}

export function actStomachSlide(m: MarioRuntime): boolean {
  if (m.actionTimer === 5) {
    if (!(m.input & INPUT_ABOVE_SLIDE) && (m.input & (INPUT_A_PRESSED | INPUT_B_PRESSED))) {
      // Forward rollouts are not ported; popping into a crouch keeps the
      // player in control without inventing new physics.
      return m.setAction(ACT_CROUCHING, 0);
    }
  } else {
    m.actionTimer++;
  }

  const held = m.action === ACT_HOLD_STOMACH_SLIDE;
  const stop = held ? ACT_DIVE_PICKING_UP : ACT_CROUCHING;
  if (updateSliding(m, 4)) return m.setAction(stop);
  commonSlideAction(m, stop, held ? ACT_HOLD_FREEFALL : ACT_FREEFALL);
  return false;
}

export function actDiveSlide(m: MarioRuntime): boolean {
  if (!(m.input & INPUT_ABOVE_SLIDE) && (m.input & (INPUT_A_PRESSED | INPUT_B_PRESSED))) {
    return m.setAction(ACT_CROUCHING, 0);
  }

  if (updateSliding(m, 8) && ++m.actionTimer > 8) {
    marioSetForwardVel(m, 0);
    return m.setAction(ACT_CROUCHING, 0);
  }

  commonSlideAction(m, ACT_CROUCHING, ACT_FREEFALL);
  return false;
}

export function actSlideKickSlide(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_PRESSED) {
    return m.setJumpingAction(ACT_FREEFALL, 2);
  }

  if (++m.actionTimer > 12 && m.forwardVel < 1) {
    return m.setAction(ACT_SLIDE_KICK_SLIDE_STOP, 0);
  }

  updateSliding(m, 1);

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      m.setAction(ACT_FREEFALL, 2);
      break;
    case GROUND_STEP_HIT_WALL:
      m.bonkReflection(true);
      m.particleFlags |= 2;
      m.setAction(ACT_DECELERATING, 0);
      break;
  }

  m.particleFlags |= 1;
  return false;
}

// ---------------------------------------------------------------------------
// Landing actions
// ---------------------------------------------------------------------------

interface LandingAction {
  numFrames: number;
  verySteepAction: number;
  endAction: number;
  offFloorAction: number;
  slideAction: number;
  aPressedAction: number;
  aPress: (m: MarioRuntime) => boolean;
}

const sJumpLandAction: LandingAction = {
  numFrames: 4,
  verySteepAction: ACT_FREEFALL,
  endAction: ACT_JUMP_LAND_STOP,
  offFloorAction: ACT_FREEFALL,
  slideAction: ACT_BEGIN_SLIDING,
  aPressedAction: ACT_DOUBLE_JUMP,
  aPress: (m) => m.setJumpingAction(ACT_DOUBLE_JUMP, 0),
};

const sFreefallLandAction: LandingAction = {
  ...sJumpLandAction,
  endAction: ACT_FREEFALL_LAND_STOP,
};

const sSideFlipLandAction: LandingAction = {
  ...sJumpLandAction,
  endAction: ACT_SIDE_FLIP_LAND_STOP,
};

const sDoubleJumpLandAction: LandingAction = {
  numFrames: 4,
  verySteepAction: ACT_FREEFALL,
  endAction: ACT_DOUBLE_JUMP_LAND_STOP,
  offFloorAction: ACT_FREEFALL,
  slideAction: ACT_BEGIN_SLIDING,
  aPressedAction: ACT_JUMP,
  aPress: (m) => setTripleJumpAction(m),
};

const sTripleJumpLandAction: LandingAction = {
  ...sDoubleJumpLandAction,
  endAction: ACT_TRIPLE_JUMP_LAND_STOP,
  aPressedAction: 0,
  aPress: (m) => m.setJumpingAction(0, 0),
};

const sLongJumpLandAction: LandingAction = {
  ...sJumpLandAction,
  numFrames: 6,
  endAction: ACT_LONG_JUMP_LAND_STOP,
  aPressedAction: ACT_JUMP,
  aPress: (m) => m.setJumpingAction(ACT_JUMP, 0),
};

const sBackflipLandAction: LandingAction = {
  ...sJumpLandAction,
  endAction: 0x0800022f, // ACT_BACKFLIP_LAND_STOP
  aPressedAction: 0x01000883, // ACT_BACKFLIP
  aPress: (m) => m.setJumpingAction(0x01000883, 0),
};

/** common_landing_cancels */
function commonLandingCancels(m: MarioRuntime, landing: LandingAction): boolean {
  if (m.floor && m.floor.normal[1] < 0.2923717) {
    return m.pushOffSteepFloor(landing.verySteepAction, 0);
  }

  m.doubleJumpTimer = 5;

  if (shouldBeginSliding(m)) {
    return m.setAction(landing.slideAction, 0);
  }

  if (++m.actionTimer >= landing.numFrames) {
    return m.setAction(landing.endAction, 0);
  }

  if (m.input & INPUT_A_PRESSED) {
    return landing.aPress(m);
  }

  if (m.input & INPUT_OFF_FLOOR) {
    return m.setAction(landing.offFloorAction, 0);
  }

  return false;
}

/** common_landing_action */
function commonLandingAction(m: MarioRuntime, airAction: number): void {
  if (m.input & INPUT_NONZERO_ANALOG) {
    applyLandingAccel(m, 0.98);
  } else if (m.forwardVel >= 16) {
    applySlopeDecel(m, 2);
  } else {
    m.vel[1] = 0;
  }

  const stepResult = m.stepGround();

  if (stepResult === GROUND_STEP_LEFT_GROUND) {
    m.setAction(airAction, 0);
  }

  if (m.forwardVel > 16) m.particleFlags |= 1;
}

export function actJumpLand(m: MarioRuntime): boolean {
  if (commonLandingCancels(m, sJumpLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actFreefallLand(m: MarioRuntime): boolean {
  if (commonLandingCancels(m, sFreefallLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actSideFlipLand(m: MarioRuntime): boolean {
  if (commonLandingCancels(m, sSideFlipLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actDoubleJumpLand(m: MarioRuntime): boolean {
  if (commonLandingCancels(m, sDoubleJumpLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actTripleJumpLand(m: MarioRuntime): boolean {
  m.input &= ~INPUT_A_PRESSED;
  if (commonLandingCancels(m, sTripleJumpLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actLongJumpLand(m: MarioRuntime): boolean {
  if (!(m.input & INPUT_Z_DOWN)) {
    m.input &= ~INPUT_A_PRESSED;
  }
  if (commonLandingCancels(m, sLongJumpLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

export function actBackflipLand(m: MarioRuntime): boolean {
  if (!(m.input & INPUT_Z_DOWN)) {
    m.input &= ~INPUT_A_PRESSED;
  }
  if (commonLandingCancels(m, sBackflipLandAction)) return true;
  commonLandingAction(m, ACT_FREEFALL);
  return false;
}

function holdThrowAction(m: MarioRuntime): boolean {
  return m.setAction(
    m.heldObj && m.heldObj.interactSubtype & INT_SUBTYPE_HOLDABLE_NPC
      ? ACT_PLACING_DOWN
      : ACT_THROWING,
    0,
  );
}

export function actHoldWalking(m: MarioRuntime): boolean {
  if (m.heldObj === null) return m.setAction(ACT_WALKING, 0);
  if (shouldBeginSliding(m)) return m.setAction(ACT_HOLD_BEGIN_SLIDING);
  if (m.input & INPUT_B_PRESSED) return holdThrowAction(m);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_HOLD_JUMP, 0);
  if (m.input & INPUT_UNKNOWN_5) return m.setAction(ACT_HOLD_DECELERATING, 0);
  if (m.input & INPUT_Z_PRESSED) {
    m.dropHeldObject();
    return m.setAction(ACT_CROUCH_SLIDE, 0);
  }

  m.intendedMag *= 0.4;
  updateWalkingSpeed(m);

  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      return m.setAction(ACT_HOLD_FREEFALL, 0);
    case GROUND_STEP_HIT_WALL:
      if (m.forwardVel > 16) marioSetForwardVel(m, 16);
      break;
  }
  return false;
}

export function actHoldDecelerating(m: MarioRuntime): boolean {
  if (m.heldObj === null) return m.setAction(ACT_DECELERATING, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_HOLD_JUMP, 0);
  if (m.input & INPUT_B_PRESSED) return holdThrowAction(m);
  if (m.input & INPUT_NONZERO_ANALOG) return m.setAction(ACT_HOLD_WALKING, 0);
  if (m.input & INPUT_OFF_FLOOR) return m.setAction(ACT_HOLD_FREEFALL, 0);
  if (shouldBeginSliding(m)) return m.setAction(ACT_HOLD_BEGIN_SLIDING);

  const stopped = applyLandingAccel(m, 0.95);
  switch (m.stepGround()) {
    case GROUND_STEP_LEFT_GROUND:
      return m.setAction(ACT_HOLD_FREEFALL, 0);
  }
  if (stopped) return m.setAction(ACT_HOLD_IDLE, 0);
  return false;
}

function holdLanding(m: MarioRuntime, fallback: number): boolean {
  if (m.heldObj === null) return m.setAction(fallback, 0);
  if (m.input & INPUT_A_PRESSED) return m.setJumpingAction(ACT_HOLD_JUMP, 0);
  if (m.input & INPUT_OFF_FLOOR) return m.setAction(ACT_HOLD_FREEFALL, 0);
  if (m.input & INPUT_NONZERO_ANALOG) return m.setAction(ACT_HOLD_WALKING, 0);
  if (m.input & INPUT_B_PRESSED) return holdThrowAction(m);

  applyLandingAccel(m, 0.9);
  if (m.stepGround() === GROUND_STEP_LEFT_GROUND) return m.setAction(ACT_HOLD_FREEFALL, 0);
  if (++m.actionTimer >= 6) return m.setAction(ACT_HOLD_IDLE, 0);
  return false;
}

export function actHoldJumpLand(m: MarioRuntime): boolean {
  return holdLanding(m, ACT_JUMP_LAND);
}

export function actHoldFreefallLand(m: MarioRuntime): boolean {
  return holdLanding(m, ACT_FREEFALL_LAND);
}

/** Dispatch table for the moving group. */
function groundKnockback(m: MarioRuntime): boolean {
  const soft = m.action === ACT_SOFT_BACKWARD_GROUND_KB || m.action === ACT_SOFT_FORWARD_GROUND_KB;
  const hard = m.action === ACT_HARD_BACKWARD_GROUND_KB || m.action === ACT_HARD_FORWARD_GROUND_KB;
  const backward = m.action === ACT_SOFT_BACKWARD_GROUND_KB
    || m.action === ACT_BACKWARD_GROUND_KB
    || m.action === ACT_HARD_BACKWARD_GROUND_KB;
  const duration = soft ? 24 : hard ? (backward ? 70 : 30) : (backward ? 30 : 30);
  m.forwardVel = Math.max(-32, Math.min(32, m.forwardVel));
  if (soft || m.actionTimer < (backward ? 22 : 20)) applyLandingAccel(m, 0.9);
  else marioSetForwardVel(m, m.forwardVel < 0 ? -0.1 : 0.1);
  if (m.stepGround() === GROUND_STEP_LEFT_GROUND) {
    if (m.forwardVel >= 0) m.setAction(hard ? ACT_HARD_FORWARD_AIR_KB : ACT_FORWARD_AIR_KB, m.actionArg);
    else m.setAction(hard ? ACT_HARD_BACKWARD_AIR_KB : ACT_BACKWARD_AIR_KB, m.actionArg);
  } else if (++m.actionTimer >= duration) {
    if (m.actionArg > 0) m.invincTimer = 30;
    m.setAction(ACT_IDLE);
  }
  return false;
}

/** update_shell_speed and act_riding_shell_ground. */
function actRidingShellGround(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_PRESSED) return m.setAction(ACT_RIDING_SHELL_JUMP);
  if (m.input & INPUT_Z_PRESSED) {
    if (m.forwardVel<24) marioSetForwardVel(m,24);
    return m.setAction(ACT_CROUCH_SLIDE);
  }
  const target = Math.max(24,Math.min(m.floor?.type===SURFACE_SLOW ? 48 : 64,m.intendedMag*2));
  if (m.forwardVel<=0) m.forwardVel += 1.1;
  else if (m.forwardVel<=target) m.forwardVel += 1.1-m.forwardVel/58;
  else if (m.floor && m.floor.normal[1]>=0.95) m.forwardVel--;
  m.forwardVel = Math.min(m.forwardVel,64);
  const startYaw = m.faceAngle[1];
  m.faceAngle[1] = toS16(m.intendedYaw-approachS32(toS16(m.intendedYaw-m.faceAngle[1]),0,0x800,0x800));
  applySlopeAccel(m);
  const lean = -Math.trunc(toS16(m.faceAngle[1]-startYaw)*m.forwardVel/12);
  m.shellRoll = approachS32(m.shellRoll,Math.max(-0x1800,Math.min(0x1800,lean)),0x200,0x200);
  const step = m.stepGround();
  if (step===GROUND_STEP_LEFT_GROUND) m.setAction(ACT_RIDING_SHELL_FALL);
  if (step===GROUND_STEP_HIT_WALL) {
    m.events.push({type:'bonk',speed:m.forwardVel});
    m.setAction(ACT_BACKWARD_GROUND_KB);
  }
  return false;
}

export const MOVING_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_SOFT_BACKWARD_GROUND_KB]: groundKnockback,
  [ACT_SOFT_FORWARD_GROUND_KB]: groundKnockback,
  [ACT_BACKWARD_GROUND_KB]: groundKnockback,
  [ACT_FORWARD_GROUND_KB]: groundKnockback,
  [ACT_HARD_BACKWARD_GROUND_KB]: groundKnockback,
  [ACT_HARD_FORWARD_GROUND_KB]: groundKnockback,
  [ACT_RIDING_SHELL_GROUND]: actRidingShellGround,
  [ACT_WALKING]: actWalking,
  [ACT_TURNING_AROUND]: actTurningAround,
  [ACT_FINISH_TURNING_AROUND]: actFinishTurningAround,
  [ACT_BRAKING]: actBraking,
  [ACT_DECELERATING]: actDecelerating,
  [ACT_CRAWLING]: actCrawling,
  [ACT_CROUCH_SLIDE]: actCrouchSlide,
  [ACT_HOLD_STOMACH_SLIDE]: actStomachSlide,
  [ACT_HOLD_BUTT_SLIDE]: m => commonSlideActionWithJump(m,ACT_HOLD_BUTT_SLIDE_STOP,ACT_HOLD_JUMP,ACT_HOLD_BUTT_SLIDE_AIR),
  [ACT_BUTT_SLIDE]: actButtSlide,
  [ACT_STOMACH_SLIDE]: actStomachSlide,
  [ACT_DIVE_SLIDE]: actDiveSlide,
  [ACT_SLIDE_KICK_SLIDE]: actSlideKickSlide,
  [ACT_MOVE_PUNCHING]: actMovePunching,
  [ACT_JUMP_LAND]: actJumpLand,
  [ACT_FREEFALL_LAND]: actFreefallLand,
  [ACT_DOUBLE_JUMP_LAND]: actDoubleJumpLand,
  [ACT_SIDE_FLIP_LAND]: actSideFlipLand,
  [ACT_TRIPLE_JUMP_LAND]: actTripleJumpLand,
  [ACT_LONG_JUMP_LAND]: actLongJumpLand,
  [ACT_BACKFLIP_LAND]: actBackflipLand,
  [ACT_HOLD_WALKING]: actHoldWalking,
  [ACT_HOLD_DECELERATING]: actHoldDecelerating,
  [ACT_HOLD_JUMP_LAND]: actHoldJumpLand,
  [ACT_HOLD_FREEFALL_LAND]: actHoldFreefallLand,
};
