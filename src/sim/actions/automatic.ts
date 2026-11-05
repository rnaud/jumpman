/**
 * Automatic actions: ledge grabs and ledge climbs.
 * Ported from src/game/mario_actions_automatic.c.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { toS16 } from '../math';
import { stopAndSetHeightToFloor } from '../marioStep';
import {
  ACT_FREEFALL,
  ACT_GROUND_POUND,
  ACT_HANGING,
  ACT_HANG_MOVING,
  ACT_IDLE,
  ACT_LEDGE_CLIMB_FAST,
  ACT_LEDGE_CLIMB_SLOW_1,
  ACT_LEDGE_CLIMB_SLOW_2,
  ACT_LEDGE_GRAB,
  ACT_SOFT_BONK,
  ACT_START_HANGING,
  INPUT_A_DOWN,
  INPUT_A_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
  INPUT_Z_PRESSED,
} from '../state';
import { SURFACE_HANGABLE } from '../surfaces';

function sin(angle: number): number {
  return Math.sin(toS16(angle) / 32768 * Math.PI);
}
function cos(angle: number): number {
  return Math.cos(toS16(angle) / 32768 * Math.PI);
}

/** let_go_of_ledge */
export function letGoOfLedge(m: MarioRuntime): boolean {
  m.vel[1] = 0;
  m.forwardVel = -8;
  m.pos[0] -= 60 * sin(m.faceAngle[1]);
  m.pos[2] -= 60 * cos(m.faceAngle[1]);

  const floorHit = m.world.findFloor(m.pos[0], m.pos[1], m.pos[2]);
  if (floorHit.height < m.pos[1] - 100) {
    m.pos[1] -= 100;
  } else {
    m.pos[1] = floorHit.height;
    if (floorHit.surface) {
      m.floor = floorHit.surface;
      m.floorHeight = floorHit.height;
    }
  }

  return m.setAction(ACT_SOFT_BONK, 0);
}

/** act_ledge_grab */
export function actLedgeGrab(m: MarioRuntime): boolean {
  const hasSpaceForMario = m.ceilHeight - m.floorHeight >= 160;

  if (m.actionTimer < 10) m.actionTimer++;

  if (m.floor && m.floor.normal[1] < 0.9063078) {
    return letGoOfLedge(m);
  }

  if (m.input & (INPUT_Z_PRESSED | INPUT_OFF_FLOOR)) {
    return letGoOfLedge(m);
  }

  if (m.input & INPUT_A_PRESSED && hasSpaceForMario) {
    return m.setAction(ACT_LEDGE_CLIMB_FAST, 0);
  }

  if (m.actionTimer === 10 && m.input & INPUT_NONZERO_ANALOG) {
    const intendedDYaw = toS16(m.intendedYaw - m.faceAngle[1]);
    if (intendedDYaw >= -0x4000 && intendedDYaw <= 0x4000 && hasSpaceForMario) {
      return m.setAction(ACT_LEDGE_CLIMB_SLOW_1, 0);
    }
    if (intendedDYaw < -0x4000 || intendedDYaw > 0x4000) {
      return letGoOfLedge(m);
    }
  }

  const heightAboveFloor =
    m.pos[1] - m.floorHeightRelativePolar(-0x8000, 30);
  if (hasSpaceForMario && heightAboveFloor < 100) {
    return m.setAction(ACT_LEDGE_CLIMB_FAST, 0);
  }

  stopAndSetHeightToFloor(m);
  return false;
}

/** act_ledge_climb_slow */
export function actLedgeClimbSlow(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return letGoOfLedge(m);
  }

  stopAndSetHeightToFloor(m);

  if (++m.actionTimer >= 28) {
    m.pos[0] += 14 * sin(m.faceAngle[1]);
    m.pos[2] += 14 * cos(m.faceAngle[1]);
    return m.setAction(ACT_IDLE, 0);
  }

  return false;
}

/** act_ledge_climb_fast */
export function actLedgeClimbFast(m: MarioRuntime): boolean {
  if (m.input & INPUT_OFF_FLOOR) {
    return letGoOfLedge(m);
  }

  stopAndSetHeightToFloor(m);

  if (++m.actionTimer >= 12) {
    // climb_up_ledge
    m.pos[0] += 14 * sin(m.faceAngle[1]);
    m.pos[2] += 14 * cos(m.faceAngle[1]);
    return m.setAction(ACT_IDLE, 0);
  }

  return false;
}

function canHang(m: MarioRuntime): boolean {
  return m.ceil !== null && m.ceil.type === SURFACE_HANGABLE;
}

/** act_start_hanging */
export function actStartHanging(m: MarioRuntime): boolean {
  m.actionTimer++;
  if (!(m.input & INPUT_A_DOWN)) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);
  if (!canHang(m)) return m.setAction(ACT_FREEFALL, 0);

  m.animLength = 53;
  m.pos[1] = m.ceilHeight - 160;
  m.forwardVel = 0;
  m.vel = [0, 0, 0];
  if (m.animFrame >= 53 || m.actionTimer >= 54) return m.setAction(ACT_HANGING, 0);
  return false;
}

/** act_hanging */
export function actHanging(m: MarioRuntime): boolean {
  if (m.input & INPUT_NONZERO_ANALOG) return m.setAction(ACT_HANG_MOVING, m.actionArg);
  if (!(m.input & INPUT_A_DOWN)) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);
  if (!canHang(m)) return m.setAction(ACT_FREEFALL, 0);

  m.animLength = 30;
  m.pos[1] = m.ceilHeight - 160;
  m.forwardVel = 0;
  m.vel = [0, 0, 0];
  return false;
}

/** act_hang_moving */
export function actHangMoving(m: MarioRuntime): boolean {
  if (!(m.input & INPUT_A_DOWN)) return m.setAction(ACT_FREEFALL, 0);
  if (m.input & INPUT_Z_PRESSED) return m.setAction(ACT_GROUND_POUND, 0);
  if (!canHang(m)) return m.setAction(ACT_FREEFALL, 0);

  m.actionTimer++;
  m.animLength = 30;
  m.forwardVel = Math.min(4, m.forwardVel + 1);
  m.faceAngle[1] = m.intendedYaw;
  m.pos[0] += m.forwardVel * sin(m.faceAngle[1]);
  m.pos[2] += m.forwardVel * cos(m.faceAngle[1]);
  m.pos[1] = m.ceilHeight - 160;
  m.vel = [0, 0, 0];

  if (!(m.input & INPUT_NONZERO_ANALOG)) return m.setAction(ACT_HANGING, m.actionArg ^ 1);
  return false;
}

export const AUTOMATIC_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_LEDGE_GRAB]: actLedgeGrab,
  [ACT_LEDGE_CLIMB_SLOW_1]: actLedgeClimbSlow,
  [ACT_LEDGE_CLIMB_SLOW_2]: actLedgeClimbSlow,
  [ACT_LEDGE_CLIMB_FAST]: actLedgeClimbFast,
  [ACT_START_HANGING]: actStartHanging,
  [ACT_HANGING]: actHanging,
  [ACT_HANG_MOVING]: actHangMoving,
};
