/**
 * Automatic actions: ledge grabs and ledge climbs.
 * Ported from src/game/mario_actions_automatic.c.
 */

import { inCannon } from './cannon';
import { ACT_IN_CANNON } from '../state';
import type { MarioRuntime } from '../MarioRuntime';
import { toS16 } from '../math';
import { stopAndSetHeightToFloor } from '../marioStep';
import {
  ACT_CLIMBING_POLE,
  ACT_FREEFALL,
  ACT_GRAB_POLE_FAST,
  ACT_GRAB_POLE_SLOW,
  ACT_GROUND_POUND,
  ACT_HOLDING_POLE,
  ACT_JUMP,
  ACT_TOP_OF_POLE,
  ACT_TOP_OF_POLE_TRANSITION,
  ACT_WALL_KICK_AIR,
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

function poleTop(m: MarioRuntime): number {
  return m.usedObj ? m.usedObj.hitbox.height - 100 : 0;
}

function setPolePosition(m: MarioRuntime): boolean {
  const pole = m.usedObj;
  if (!pole || !pole.active) return m.setAction(ACT_FREEFALL, 0);
  m.polePos = Math.max(0, Math.min(poleTop(m), m.polePos));
  m.pos[0] = pole.pos[0];
  m.pos[1] = pole.pos[1] + m.polePos;
  m.pos[2] = pole.pos[2];
  m.forwardVel = 0;
  m.vel = [0, 0, 0];
  return true;
}

function leavePole(m: MarioRuntime, action: number): boolean {
  m.faceAngle[0] = 0;
  return m.setAction(action, 0);
}

/** act_grab_pole_slow / act_grab_pole_fast */
export function actGrabPole(m: MarioRuntime): boolean {
  m.animLength = 15;
  if (!setPolePosition(m)) return true;
  if (m.animFrame >= 15) return m.setAction(ACT_HOLDING_POLE, 0);
  return false;
}

/** act_holding_pole */
export function actHoldingPole(m: MarioRuntime): boolean {
  if (m.input & INPUT_Z_PRESSED || m.health < 0x100) {
    m.forwardVel = -2;
    return leavePole(m, ACT_SOFT_BONK);
  }
  if (m.input & INPUT_A_PRESSED) {
    m.faceAngle[1] = (m.faceAngle[1] + 0x8000) & 0xffff;
    return leavePole(m, ACT_WALL_KICK_AIR);
  }

  if (m.stickY > 16) {
    if (m.polePos < poleTop(m) - 0.4) return m.setAction(ACT_CLIMBING_POLE, 0);
    if (m.stickY > 50) return m.setAction(ACT_TOP_OF_POLE_TRANSITION, 0);
  } else if (m.stickY < -16) {
    m.polePos -= 2;
  }

  m.animLength = 30;
  if (!setPolePosition(m)) return true;
  return false;
}

/** act_climbing_pole */
export function actClimbingPole(m: MarioRuntime): boolean {
  if (m.input & INPUT_A_PRESSED) {
    m.faceAngle[1] = (m.faceAngle[1] + 0x8000) & 0xffff;
    return leavePole(m, ACT_WALL_KICK_AIR);
  }
  if (m.stickY < 8) return m.setAction(ACT_HOLDING_POLE, 0);

  m.polePos += m.stickY / 8;
  if (m.polePos >= poleTop(m)) {
    m.polePos = poleTop(m);
    return m.setAction(ACT_TOP_OF_POLE_TRANSITION, 0);
  }

  m.animLength = 20;
  if (!setPolePosition(m)) return true;
  return false;
}

/** act_top_of_pole_transition */
export function actTopOfPoleTransition(m: MarioRuntime): boolean {
  m.polePos = poleTop(m);
  m.animLength = 19;
  if (!setPolePosition(m)) return true;
  if (m.animFrame >= 19) return m.setAction(ACT_TOP_OF_POLE, 0);
  return false;
}

/** act_top_of_pole */
export function actTopOfPole(m: MarioRuntime): boolean {
  m.polePos = poleTop(m);
  if (!setPolePosition(m)) return true;

  if (m.input & INPUT_A_PRESSED) {
    m.vel = [0, 60, 0];
    return m.setAction(ACT_JUMP, 0);
  }
  if (m.stickY < -16) return m.setAction(ACT_TOP_OF_POLE_TRANSITION, 1);

  m.animLength = 30;
  return false;
}

export const AUTOMATIC_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_IN_CANNON]: inCannon,
  [ACT_LEDGE_GRAB]: actLedgeGrab,
  [ACT_LEDGE_CLIMB_SLOW_1]: actLedgeClimbSlow,
  [ACT_LEDGE_CLIMB_SLOW_2]: actLedgeClimbSlow,
  [ACT_LEDGE_CLIMB_FAST]: actLedgeClimbFast,
  [ACT_START_HANGING]: actStartHanging,
  [ACT_HANGING]: actHanging,
  [ACT_HANG_MOVING]: actHangMoving,
  [ACT_GRAB_POLE_SLOW]: actGrabPole,
  [ACT_GRAB_POLE_FAST]: actGrabPole,
  [ACT_HOLDING_POLE]: actHoldingPole,
  [ACT_CLIMBING_POLE]: actClimbingPole,
  [ACT_TOP_OF_POLE_TRANSITION]: actTopOfPoleTransition,
  [ACT_TOP_OF_POLE]: actTopOfPole,
};
