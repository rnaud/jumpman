/**
 * Airborne actions, ported from src/game/mario_actions_airborne.c.
 *
 * The feel of SM64's jumps lives here: variable jump height (holding A),
 * double/triple jumps, backflips, side flips, long jumps, dives, ground
 * pounds, wall kicks and ledge grabs.
 */

import { shotFromCannon } from './cannon';
import { ACT_SHOT_FROM_CANNON } from '../state';
import type { MarioRuntime } from '../MarioRuntime';
import { SURFACE_BURNING } from '../surfaces';
import { approachF32, toS16 } from '../math';
import { marioSetForwardVel } from '../marioHelpers';
import {
  ACT_BACKWARD_AIR_KB, ACT_FORWARD_AIR_KB, ACT_BACKWARD_GROUND_KB, ACT_FORWARD_GROUND_KB,
  ACT_HARD_BACKWARD_AIR_KB, ACT_HARD_FORWARD_AIR_KB, ACT_HARD_BACKWARD_GROUND_KB, ACT_HARD_FORWARD_GROUND_KB,
  ACT_LAVA_BOOST, ACT_LAVA_BOOST_LAND, ACT_START_HANGING, ACT_WATER_JUMP, AIR_STEP_CHECK_LEDGE_GRAB, AIR_STEP_CHECK_HANG,
  AIR_STEP_GRABBED_CEILING,
  AIR_STEP_GRABBED_LEDGE,
  AIR_STEP_HIT_WALL,
  AIR_STEP_LANDED,
  AIR_STEP_NONE,
  ACT_AIR_HIT_WALL,
  ACT_AIR_THROW,
  ACT_AIR_THROW_LAND,
  ACT_BACKFLIP,
  ACT_BACKFLIP_LAND,
  ACT_BEGIN_SLIDING,
  ACT_BUTT_SLIDE,
  ACT_BUTT_SLIDE_AIR,
  ACT_DIVE,
  ACT_DIVE_SLIDE,
  ACT_DOUBLE_JUMP,
  ACT_DOUBLE_JUMP_LAND,
  ACT_FREEFALL,
  ACT_FLYING,
  ACT_RIDING_SHELL_JUMP,
  ACT_FREEFALL_LAND,
  ACT_GROUND_POUND,
  ACT_GROUND_POUND_LAND,
  ACT_HOLD_FREEFALL,
  ACT_HOLD_FREEFALL_LAND,
  ACT_HOLD_JUMP,
  ACT_HOLD_JUMP_LAND,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_JUMP_LAND,
  ACT_LEDGE_GRAB,
  ACT_LONG_JUMP,
  ACT_LONG_JUMP_LAND,
  ACT_SIDE_FLIP,
  ACT_SIDE_FLIP_LAND,
  ACT_SLIDE_KICK,
  ACT_SLIDE_KICK_SLIDE,
  ACT_SOFT_BONK,
  ACT_STEEP_JUMP,
  ACT_TRIPLE_JUMP,
  ACT_TRIPLE_JUMP_LAND,
  ACT_WALL_KICK_AIR,
  INPUT_A_PRESSED,
  INPUT_B_PRESSED,
  MARIO_WING_CAP,
  MARIO_METAL_CAP,
  INPUT_NONZERO_ANALOG,
  INPUT_Z_PRESSED,
} from '../state';

function sin(angle: number): number {
  return Math.sin(toS16(angle) / 32768 * Math.PI);
}
function cos(angle: number): number {
  return Math.cos(toS16(angle) / 32768 * Math.PI);
}

// ---------------------------------------------------------------------------
// Air control
// ---------------------------------------------------------------------------

/** update_air_without_turn */
export function updateAirWithoutTurn(m: MarioRuntime): void {
  const dragThreshold = m.action === ACT_LONG_JUMP ? 48 : 32;

  m.forwardVel = approachF32(m.forwardVel, 0, 0.35, 0.35);

  let sidewaysSpeed = 0;
  if (m.input & INPUT_NONZERO_ANALOG) {
    const intendedDYaw = toS16(m.intendedYaw - m.faceAngle[1]);
    const intendedMag = m.intendedMag / 32;

    m.forwardVel += intendedMag * cos(intendedDYaw) * 1.5;
    sidewaysSpeed = intendedMag * sin(intendedDYaw) * 10;
  }

  if (m.forwardVel > dragThreshold) m.forwardVel -= 1;
  if (m.forwardVel < -16) m.forwardVel += 2;

  m.slideVelX = m.forwardVel * sin(m.faceAngle[1]);
  m.slideVelZ = m.forwardVel * cos(m.faceAngle[1]);

  m.slideVelX += sidewaysSpeed * sin(toS16(m.faceAngle[1] + 0x4000));
  m.slideVelZ += sidewaysSpeed * cos(toS16(m.faceAngle[1] + 0x4000));

  m.vel[0] = m.slideVelX;
  m.vel[2] = m.slideVelZ;
}

/** update_air_with_turn */
export function updateAirWithTurn(m: MarioRuntime): void {
  const dragThreshold = m.action === ACT_LONG_JUMP ? 48 : 32;

  m.forwardVel = approachF32(m.forwardVel, 0, 0.35, 0.35);

  if (m.input & INPUT_NONZERO_ANALOG) {
    const intendedDYaw = toS16(m.intendedYaw - m.faceAngle[1]);
    const intendedMag = m.intendedMag / 32;

    m.forwardVel += 1.5 * cos(intendedDYaw) * intendedMag;
    m.faceAngle[1] = toS16(m.faceAngle[1] + 512 * sin(intendedDYaw) * intendedMag);
  }

  if (m.forwardVel > dragThreshold) m.forwardVel -= 1;
  if (m.forwardVel < -16) m.forwardVel += 2;

  m.vel[0] = m.slideVelX = m.forwardVel * sin(m.faceAngle[1]);
  m.vel[2] = m.slideVelZ = m.forwardVel * cos(m.faceAngle[1]);
}

// ---------------------------------------------------------------------------
// Common steps
// ---------------------------------------------------------------------------

/** check_fall_damage (no snow/sand sticking) */
export function checkFallDamage(m: MarioRuntime): boolean {
  if (!m.floor) return false;
  if (m.flags & MARIO_METAL_CAP) return false;
  if (m.vel[1] < -55) {
    const fallHeight = m.peakHeight - m.pos[1];

    if (fallHeight > 3000) {
      m.hurtCounter += 16;
      m.health -= 16;
      m.squishTimer = 30;
    } else if (fallHeight > 1150 && !m.floorIsSlippery()) {
      m.hurtCounter += 8;
      m.health -= 8;
      m.squishTimer = 30;
    }

    if (m.health < 0x100) {
      m.respawn();
      return true;
    }
  }
  return false;
}

/** check_wall_kick */
function checkWallKick(m: MarioRuntime): boolean {
  if (
    m.input & INPUT_A_PRESSED &&
    m.wallKickTimer !== 0 &&
    m.prevAction === ACT_AIR_HIT_WALL
  ) {
    m.faceAngle[1] = toS16(m.faceAngle[1] + 0x8000);
    return m.setAction(ACT_WALL_KICK_AIR, 0);
  }
  return false;
}

/** common_air_action_step */
function approachS32(value: number, target: number, increment: number, decrement: number): number {
  if (value < target) return Math.min(target, value + increment);
  if (value > target) return Math.max(target, value - decrement);
  return value;
}

/** update_flying_pitch / update_flying_yaw / update_flying from mario_actions_airborne.c */
export function updateFlying(m: MarioRuntime): void {
  const pitchTarget = -((m.stickY * (m.forwardVel / 5)) | 0);
  if (pitchTarget > 0) {
    if (m.angleVel[0] < 0) m.angleVel[0] = Math.min(0x20, m.angleVel[0] + 0x40);
    else m.angleVel[0] = approachS32(m.angleVel[0], pitchTarget, 0x20, 0x40);
  } else if (pitchTarget < 0) {
    if (m.angleVel[0] > 0) m.angleVel[0] = Math.max(-0x20, m.angleVel[0] - 0x40);
    else m.angleVel[0] = approachS32(m.angleVel[0], pitchTarget, 0x40, 0x20);
  } else {
    m.angleVel[0] = approachS32(m.angleVel[0], 0, 0x40, 0x40);
  }

  const yawTarget = -((m.stickX * (m.forwardVel / 4)) | 0);
  if (yawTarget > 0) {
    if (m.angleVel[1] < 0) m.angleVel[1] = Math.min(0x10, m.angleVel[1] + 0x40);
    else m.angleVel[1] = approachS32(m.angleVel[1], yawTarget, 0x10, 0x20);
  } else if (yawTarget < 0) {
    if (m.angleVel[1] > 0) m.angleVel[1] = Math.max(-0x10, m.angleVel[1] - 0x40);
    else m.angleVel[1] = approachS32(m.angleVel[1], yawTarget, 0x20, 0x10);
  } else {
    m.angleVel[1] = approachS32(m.angleVel[1], 0, 0x40, 0x40);
  }

  m.faceAngle[1] = toS16(m.faceAngle[1] + m.angleVel[1]);
  m.faceAngle[2] = toS16(20 * -m.angleVel[1]);

  m.forwardVel -= 2 * (m.faceAngle[0] / 0x4000) + 0.1;
  m.forwardVel -= 0.5 * (1 - cos(m.angleVel[1]));
  if (m.forwardVel < 0) m.forwardVel = 0;
  if (m.forwardVel > 16) m.faceAngle[0] += (m.forwardVel - 32) * 6;
  else if (m.forwardVel > 4) m.faceAngle[0] += (m.forwardVel - 32) * 10;
  else m.faceAngle[0] -= 0x400;

  m.faceAngle[0] += m.angleVel[0];
  m.faceAngle[0] = Math.max(-0x2aaa, Math.min(0x2aaa, m.faceAngle[0]));

  m.vel[0] = m.forwardVel * cos(m.faceAngle[0]) * sin(m.faceAngle[1]);
  m.vel[1] = m.forwardVel * sin(m.faceAngle[0]);
  m.vel[2] = m.forwardVel * cos(m.faceAngle[0]) * cos(m.faceAngle[1]);
  m.slideVelX = m.vel[0];
  m.slideVelZ = m.vel[2];
}

export function actFlying(m: MarioRuntime): boolean {
  if (!(m.flags & MARIO_WING_CAP)) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  if (m.actionState === 0) {
    m.actionState = 1;
    if (m.forwardVel < 16) marioSetForwardVel(m, 16);
  }

  updateFlying(m);

  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      return m.setAction(ACT_FREEFALL_LAND, 0);
    case AIR_STEP_HIT_WALL:
      m.setAction(ACT_BACKWARD_AIR_KB, 0);
      break;
  }
  return false;
}

export function actRidingShellJump(m: MarioRuntime): boolean {
  const step = m.stepAir(0);
  if (step === AIR_STEP_LANDED) {
    m.landEvent(m.vel[1], m.forwardVel);
    m.setAction(ACT_FREEFALL_LAND, 0);
  } else if (step === AIR_STEP_HIT_WALL) {
    m.setAction(ACT_BACKWARD_AIR_KB, 0);
  }
  return false;
}

export function commonAirActionStep(
  m: MarioRuntime,
  landAction: number,
  stepArg: number,
): number {
  updateAirWithoutTurn(m);

  const stepResult = m.stepAir(stepArg);

  switch (stepResult) {
    case AIR_STEP_NONE:
      break;

    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      if (!checkFallDamage(m)) {
        m.setAction(landAction, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      if (m.forwardVel > 16) {
        m.bonkReflection(false);
        m.faceAngle[1] = toS16(m.faceAngle[1] + 0x8000);

        if (m.wall !== null) {
          m.setAction(ACT_AIR_HIT_WALL, 0);
        } else {
          if (m.vel[1] > 0) m.vel[1] = 0;
          if (m.forwardVel >= 38) {
            m.particleFlags |= 2;
            m.setAction(ACT_SOFT_BONK, 0);
            m.forwardVel = -m.forwardVel;
          } else {
            if (m.forwardVel > 8) marioSetForwardVel(m, -8);
            m.setAction(ACT_SOFT_BONK, 0);
          }
        }
      } else {
        marioSetForwardVel(m, 0);
      }
      break;

    case AIR_STEP_GRABBED_LEDGE:
      m.dropAndSetAction(ACT_LEDGE_GRAB, 0);
      break;

    case AIR_STEP_GRABBED_CEILING:
      m.setAction(ACT_START_HANGING, 0);
      break;
  }

  return stepResult;
}

/** common_air_knockback_step */
function commonAirKnockbackStep(m: MarioRuntime, landAction: number, speed: number): number {
  marioSetForwardVel(m, speed);

  const stepResult = m.stepAir(0);

  switch (stepResult) {
    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      if (!checkFallDamage(m)) {
        m.setAction(landAction, m.actionArg);
      }
      break;

    case AIR_STEP_HIT_WALL:
      m.bonkReflection(false);
      if (m.vel[1] > 0) m.vel[1] = 0;
      marioSetForwardVel(m, -speed);
      break;
  }

  return stepResult;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/** check_kick_or_dive_in_air */
function checkKickOrDiveInAir(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) {
    return m.setAction(m.forwardVel > 28 ? ACT_DIVE : ACT_JUMP_KICK, 0);
  }
  return false;
}

export function actLavaBoost(m: MarioRuntime): boolean {
  m.animLength = 20;
  if (!(m.input & INPUT_NONZERO_ANALOG)) m.forwardVel *= 0.9;

  const result = m.stepAir(0);
  if (result === AIR_STEP_LANDED) {
    if (m.floor?.type === SURFACE_BURNING) {
      m.actionState = 0;
      m.hurtCounter += 18;
      m.vel[1] = 84;
    } else if (m.actionState < 2 && m.vel[1] < 0) {
      m.vel[1] = -m.vel[1] * 0.4;
      marioSetForwardVel(m, m.forwardVel * 0.5);
      m.actionState++;
    } else {
      return m.setAction(ACT_LAVA_BOOST_LAND, 0);
    }
  } else if (result === AIR_STEP_HIT_WALL) {
    marioSetForwardVel(m, -m.forwardVel);
  }
  return false;
}

export function actJump(m: MarioRuntime): boolean {
  if (checkKickOrDiveInAir(m)) return true;
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_JUMP_LAND, m.airStepArg() | AIR_STEP_CHECK_HANG);
  return false;
}

export function actDoubleJump(m: MarioRuntime): boolean {
  if (checkKickOrDiveInAir(m)) return true;
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_DOUBLE_JUMP_LAND, m.airStepArg() | AIR_STEP_CHECK_HANG);
  return false;
}

export function actTripleJump(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_TRIPLE_JUMP_LAND, 0);
  return false;
}

export function actBackflip(m: MarioRuntime): boolean {
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);
  commonAirActionStep(m, ACT_BACKFLIP_LAND, 0);
  return false;
}

export function actFreefall(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_FREEFALL_LAND, m.airStepArg());
  return false;
}

export function actSideFlip(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_SIDE_FLIP_LAND, m.airStepArg());
  return false;
}

export function actWallKickAir(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);

  commonAirActionStep(m, ACT_JUMP_LAND, m.airStepArg());
  return false;
}

export function actLongJump(m: MarioRuntime): boolean {
  commonAirActionStep(m, ACT_LONG_JUMP_LAND, m.airStepArg());
  return false;
}

export function actDive(m: MarioRuntime): boolean {
  updateAirWithoutTurn(m);

  const stepResult = m.stepAir(0);

  switch (stepResult) {
    case AIR_STEP_NONE:
      if (m.vel[1] < 0 && m.faceAngle[0] > -0x2aaa) {
        m.faceAngle[0] -= 0x200;
        if (m.faceAngle[0] < -0x2aaa) m.faceAngle[0] = -0x2aaa;
      }
      break;

    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      m.faceAngle[0] = 0;
      if (!checkFallDamage(m)) {
        m.setAction(ACT_DIVE_SLIDE, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      m.bonkReflection(true);
      m.faceAngle[0] = 0;
      if (m.vel[1] > 0) m.vel[1] = 0;
      m.particleFlags |= 2;
      marioSetForwardVel(m, -16);
      m.setAction(ACT_SOFT_BONK, 0);
      break;
  }

  return false;
}

export function actGroundPound(m: MarioRuntime): boolean {
  if (m.actionState === 0) {
    if (m.actionTimer < 10) {
      const yOffset = 20 - 2 * m.actionTimer;
      if (m.pos[1] + yOffset + 160 < m.ceilHeight) {
        m.pos[1] += yOffset;
        m.peakHeight = m.pos[1];
      }
    }

    m.vel[1] = -50;
    marioSetForwardVel(m, 0);

    m.actionTimer++;
    if (m.actionTimer >= (m.actionArg === 0 ? 11 : 16) + 4) {
      m.events.push({ type: 'groundpounddrop' });
      m.actionState = 1;
    }
    return false;
  }

  const stepResult = m.stepAir(0);

  if (stepResult === AIR_STEP_LANDED) {
    m.landEvent(m.vel[1], m.forwardVel);
    if (!checkFallDamage(m)) {
      m.particleFlags |= 3;
      m.setAction(ACT_GROUND_POUND_LAND, 0);
    }
  } else if (stepResult === AIR_STEP_HIT_WALL) {
    marioSetForwardVel(m, -16);
    if (m.vel[1] > 0) m.vel[1] = 0;
    m.particleFlags |= 2;
    m.setAction(ACT_SOFT_BONK, 0);
  }

  return false;
}

export function actSteepJump(m: MarioRuntime): boolean {
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_DIVE, 0);

  marioSetForwardVel(m, 0.98 * m.forwardVel);

  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      m.faceAngle[0] = 0;
      if (!checkFallDamage(m)) {
        m.setAction(m.forwardVel < 0 ? ACT_BEGIN_SLIDING : ACT_JUMP_LAND, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      marioSetForwardVel(m, 0);
      break;
  }

  return false;
}

export function actAirHitWall(m: MarioRuntime): boolean {
  if (++m.actionTimer <= 2) {
    if (m.input & INPUT_A_PRESSED) {
      m.vel[1] = 52;
      m.faceAngle[1] = toS16(m.faceAngle[1] + 0x8000);
      return m.setAction(ACT_WALL_KICK_AIR, 0);
    }
  } else if (m.forwardVel >= 38) {
    m.wallKickTimer = 5;
    if (m.vel[1] > 0) m.vel[1] = 0;
    m.particleFlags |= 2;
    marioSetForwardVel(m, -m.forwardVel);
    return m.setAction(ACT_SOFT_BONK, 0);
  } else {
    m.wallKickTimer = 5;
    if (m.vel[1] > 0) m.vel[1] = 0;
    if (m.forwardVel > 8) marioSetForwardVel(m, -8);
    return m.setAction(ACT_SOFT_BONK, 0);
  }

  return false;
}

export function actSoftBonk(m: MarioRuntime): boolean {
  if (checkWallKick(m)) return true;

  const speed = m.forwardVel;
  commonAirKnockbackStep(m, ACT_FREEFALL_LAND, speed);
  return false;
}

export function actSlideKick(m: MarioRuntime): boolean {
  if (++m.actionTimer > 30 && m.pos[1] - m.floorHeight > 500) {
    return m.setAction(ACT_FREEFALL, 2);
  }

  updateAirWithoutTurn(m);

  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      if (m.actionState === 0 && m.vel[1] < 0) {
        m.vel[1] = -m.vel[1] / 2;
        m.actionState = 1;
        m.actionTimer = 0;
        m.landEvent(0, m.forwardVel);
      } else {
        m.landEvent(m.vel[1], m.forwardVel);
        m.setAction(ACT_SLIDE_KICK_SLIDE, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      if (m.vel[1] > 0) m.vel[1] = 0;
      m.particleFlags |= 2;
      marioSetForwardVel(m, -16);
      m.setAction(ACT_SOFT_BONK, 0);
      break;
  }

  return false;
}

export function actJumpKick(m: MarioRuntime): boolean {
  updateAirWithoutTurn(m);

  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      m.landEvent(m.vel[1], m.forwardVel);
      if (!checkFallDamage(m)) {
        m.setAction(ACT_FREEFALL_LAND, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      marioSetForwardVel(m, 0);
      break;
  }

  return false;
}

export function actButtSlideAir(m: MarioRuntime): boolean {
  if (++m.actionTimer > 30 && m.pos[1] - m.floorHeight > 500) {
    return m.setAction(ACT_FREEFALL, 1);
  }

  updateAirWithTurn(m);

  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      if (m.actionState === 0 && m.vel[1] < 0 && m.floor && m.floor.normal[1] >= 0.9848077) {
        m.vel[1] = -m.vel[1] / 2;
        m.actionState = 1;
        m.landEvent(0, m.forwardVel);
      } else {
        m.landEvent(m.vel[1], m.forwardVel);
        m.setAction(ACT_BUTT_SLIDE, 0);
      }
      break;

    case AIR_STEP_HIT_WALL:
      if (m.vel[1] > 0) m.vel[1] = 0;
      m.particleFlags |= 2;
      marioSetForwardVel(m, -8);
      m.setAction(ACT_SOFT_BONK, 0);
      break;
  }

  return false;
}

function airKnockback(m: MarioRuntime): boolean {
  const hard = m.action === ACT_HARD_BACKWARD_AIR_KB || m.action === ACT_HARD_FORWARD_AIR_KB;
  const backward = m.action === ACT_BACKWARD_AIR_KB || m.action === ACT_HARD_BACKWARD_AIR_KB;
  const speed = backward ? -16 : 16;
  marioSetForwardVel(m, speed);
  switch (m.stepAir(0)) {
    case AIR_STEP_LANDED:
      m.setAction(
        backward
          ? (hard ? ACT_HARD_BACKWARD_GROUND_KB : ACT_BACKWARD_GROUND_KB)
          : (hard ? ACT_HARD_FORWARD_GROUND_KB : ACT_FORWARD_GROUND_KB),
        m.actionArg,
      );
      break;
    case AIR_STEP_HIT_WALL:
      m.bonkReflection(false);
      m.vel[1] = Math.min(0, m.vel[1]);
      marioSetForwardVel(m, -speed);
      break;
  }
  return false;
}

export function actHoldJump(m: MarioRuntime): boolean {
  if (m.heldObj === null) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_AIR_THROW, 0);
  if (m.input & INPUT_Z_PRESSED) {
    m.dropHeldObject();
    return m.setAction(ACT_GROUND_POUND, 0);
  }
  commonAirActionStep(m, ACT_HOLD_JUMP_LAND, AIR_STEP_CHECK_LEDGE_GRAB);
  return false;
}

export function actHoldFreefall(m: MarioRuntime): boolean {
  if (m.heldObj === null) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_B_PRESSED) return m.setAction(ACT_AIR_THROW, 0);
  if (m.input & INPUT_Z_PRESSED) {
    m.dropHeldObject();
    return m.setAction(ACT_GROUND_POUND, 0);
  }
  commonAirActionStep(m, ACT_HOLD_FREEFALL_LAND, AIR_STEP_CHECK_LEDGE_GRAB);
  return false;
}

export function actAirThrow(m: MarioRuntime): boolean {
  if (m.heldObj === null) return m.setAction(ACT_FREEFALL, 0);
  if (++m.actionTimer === 4) m.throwHeldObject();
  commonAirActionStep(m, ACT_AIR_THROW_LAND, 0);
  return false;
}

/** Dispatch table for the airborne group. */
export const AIRBORNE_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_SHOT_FROM_CANNON]: shotFromCannon,
  [ACT_BACKWARD_AIR_KB]: airKnockback,
  [ACT_FORWARD_AIR_KB]: airKnockback,
  [ACT_HARD_BACKWARD_AIR_KB]: airKnockback,
  [ACT_HARD_FORWARD_AIR_KB]: airKnockback,
  [ACT_WATER_JUMP]: m => {
    if (m.forwardVel < 15) marioSetForwardVel(m, 15);
    switch (m.stepAir(AIR_STEP_CHECK_LEDGE_GRAB)) {
      case AIR_STEP_LANDED: m.setAction(ACT_JUMP_LAND); break;
      case AIR_STEP_HIT_WALL: marioSetForwardVel(m, 15); break;
      case AIR_STEP_GRABBED_LEDGE: m.setAction(ACT_LEDGE_GRAB); break;
    }
    return false;
  },
  [ACT_FLYING]: actFlying,
  [ACT_RIDING_SHELL_JUMP]: actRidingShellJump,
  [ACT_JUMP]: actJump,
  [ACT_DOUBLE_JUMP]: actDoubleJump,
  [ACT_TRIPLE_JUMP]: actTripleJump,
  [ACT_BACKFLIP]: actBackflip,
  [ACT_FREEFALL]: actFreefall,
  [ACT_SIDE_FLIP]: actSideFlip,
  [ACT_WALL_KICK_AIR]: actWallKickAir,
  [ACT_LONG_JUMP]: actLongJump,
  [ACT_DIVE]: actDive,
  [ACT_GROUND_POUND]: actGroundPound,
  [ACT_LAVA_BOOST]: actLavaBoost,
  [ACT_STEEP_JUMP]: actSteepJump,
  [ACT_AIR_HIT_WALL]: actAirHitWall,
  [ACT_SOFT_BONK]: actSoftBonk,
  [ACT_SLIDE_KICK]: actSlideKick,
  [ACT_JUMP_KICK]: actJumpKick,
  [ACT_BUTT_SLIDE_AIR]: actButtSlideAir,
  [ACT_HOLD_JUMP]: actHoldJump,
  [ACT_HOLD_FREEFALL]: actHoldFreefall,
  [ACT_AIR_THROW]: actAirThrow,
};
