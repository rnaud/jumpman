/**
 * Automatic actions: ledge grabs and ledge climbs.
 * Ported from src/game/mario_actions_automatic.c.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { toS16 } from '../math';
import { stopAndSetHeightToFloor } from '../marioStep';
import {
  ACT_IDLE,
  ACT_LEDGE_CLIMB_FAST,
  ACT_LEDGE_GRAB,
  ACT_SOFT_BONK,
  INPUT_A_PRESSED,
  INPUT_NONZERO_ANALOG,
  INPUT_OFF_FLOOR,
  INPUT_Z_PRESSED,
} from '../state';

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
      return m.setAction(ACT_LEDGE_CLIMB_FAST, 0);
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

export const AUTOMATIC_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_LEDGE_GRAB]: actLedgeGrab,
  [ACT_LEDGE_CLIMB_FAST]: actLedgeClimbFast,
};
