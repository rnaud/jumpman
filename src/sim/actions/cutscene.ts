/**
 * Cutscene-group death actions, ported from
 * reference/sm64/src/game/mario_actions_cutscene.c. The source triggers a
 * level warp after a fixed animation frame; Jumpman respawns at that frame.
 */

import { atan2s, clamp, toS16 } from '../math';
import { jumboStarCutscene, endPeachCutscene } from './ending';
import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_SPAWN_NO_SPIN_AIRBORNE, ACT_SPAWN_NO_SPIN_LANDING,
  ACT_FALL_AFTER_STAR_GRAB, ACT_STAR_DANCE_EXIT, ACT_STAR_DANCE_WATER,
  ACT_GROUP_MASK, ACT_GROUP_MOVING, ACT_GROUP_STATIONARY, ACT_FLAG_RIDING_SHELL, ACT_FLAG_INVULNERABLE, ACT_READING_SIGN, ACT_READING_NPC_DIALOG, ACT_HOLD_IDLE,
  ACT_END_PEACH_CUTSCENE, ACT_JUMBO_STAR_CUTSCENE, ACT_IDLE, ACT_SPAWN_SPIN_AIRBORNE, ACT_SPAWN_SPIN_LANDING,
  ACT_SPECIAL_EXIT_AIRBORNE, ACT_EXIT_AIRBORNE, ACT_FALLING_EXIT_AIRBORNE, ACT_EXIT_LAND_SAVE_DIALOG,
  AIR_STEP_LANDED,
  ACT_TELEPORT_FADE_IN, ACT_TELEPORT_FADE_OUT, ACT_WATER_IDLE,
  ACT_DEATH_ON_BACK,
  ACT_DEATH_ON_STOMACH,
  ACT_QUICKSAND_DEATH,
  ACT_STANDING_DEATH,
} from '../state';
import { marioSetForwardVel, stopAndSetHeightToFloor } from '../marioStep';

function deathFrame(m: MarioRuntime, frames: number): boolean {
  stopAndSetHeightToFloor(m);
  m.animLength = frames;
  if (++m.actionTimer >= frames) {
    m.respawn();
  }
  return false;
}

export function actQuicksandDeath(m: MarioRuntime): boolean {
  stopAndSetHeightToFloor(m);
  m.animLength = 30;
  m.quicksandDepth += 5;
  if (m.quicksandDepth >= 180) m.respawn();
  return false;
}

export function actStandingDeath(m: MarioRuntime): boolean {
  return deathFrame(m, 80);
}

export function actDeathOnBack(m: MarioRuntime): boolean {
  return deathFrame(m, 54);
}

export function actDeathOnStomach(m: MarioRuntime): boolean {
  return deathFrame(m, 37);
}

function spawnSpinAirborne(m: MarioRuntime): boolean {
  marioSetForwardVel(m, m.forwardVel);
  if (m.stepAir() === AIR_STEP_LANDED) {
    m.setAction(ACT_SPAWN_SPIN_LANDING);
  } else if (m.pos[1] - m.floorHeight <= 300) {
    m.actionState = 1;
  }
  return false;
}

function starExitAirborne(m: MarioRuntime): boolean {
  const painting = m.action === ACT_EXIT_AIRBORNE;
  const launch = m.action === ACT_SPECIAL_EXIT_AIRBORNE;
  if ((painting && m.actionTimer++ <= 15) || (launch && m.actionTimer++ < 11)) return false;
  marioSetForwardVel(m, painting ? -32 : launch ? -24 : 0);
  if (m.stepAir() === AIR_STEP_LANDED) {
    m.setAction(ACT_EXIT_LAND_SAVE_DIALOG, launch ? 1 : 0);
    m.healCounter = 31;
  }
  return false;
}

function warpLanding(m: MarioRuntime): boolean {
  stopAndSetHeightToFloor(m);
  // GENERAL_LAND plays source frames 9 through 23.
  if (++m.actionTimer >= 15) m.setAction(ACT_IDLE);
  return false;
}

export function readyForNpcDialog(m: MarioRuntime): boolean {
  const group = m.action & ACT_GROUP_MASK;
  return (group === ACT_GROUP_STATIONARY || group === ACT_GROUP_MOVING)
    && !(m.action & (ACT_FLAG_RIDING_SHELL | ACT_FLAG_INVULNERABLE));
}

function readingNpcDialog(m: MarioRuntime): boolean {
  const headTurn = m.actionArg === 2 ? -1024 : m.actionArg === 3 ? 384 : 0;
  if (m.actionState < 8 && m.usedObj) {
    const target = atan2s(m.usedObj.pos[2]-m.pos[2], m.usedObj.pos[0]-m.pos[0]);
    m.faceAngle[1] = toS16(m.faceAngle[1] + clamp(toS16(target-m.faceAngle[1]), -2048, 2048));
    m.actionTimer += headTurn;
  } else if (m.actionState >= 9 && m.actionState < 17) {
    m.actionTimer -= headTurn;
  } else if (m.actionState === 23) {
    m.setAction(m.heldObj ? ACT_HOLD_IDLE : ACT_IDLE);
    return false;
  }
  if (m.actionState !== 8) m.actionState++;
  return false;
}

export const CUTSCENE_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_FALL_AFTER_STAR_GRAB]: m => {
    if (m.pos[1] < m.world.waterLevel(m.pos[0],m.pos[2]) - 130) return m.setAction(ACT_STAR_DANCE_WATER);
    if (m.stepAir(1) === AIR_STEP_LANDED) m.setAction(ACT_STAR_DANCE_EXIT);
    return false;
  },
  [ACT_STAR_DANCE_EXIT]: m => {
    stopAndSetHeightToFloor(m);
    m.faceAngle[1] = m.cameraYaw;
    m.actionTimer++;
    return false;
  },
  [ACT_STAR_DANCE_WATER]: m => {
    m.faceAngle[1] = m.cameraYaw;
    m.actionTimer++;
    return false;
  },
  [ACT_READING_SIGN]: m => {
    if (m.actionState !== 2) {
      m.actionState = 1;
      m.pos[0] += m.signMotion[0];
      m.faceAngle[1] = toS16(m.faceAngle[1]+m.signMotion[1]);
      m.pos[2] += m.signMotion[2];
      if (++m.actionTimer === 11) m.actionState = 2;
    } else m.actionTimer++;
    return false;
  },
  [ACT_READING_NPC_DIALOG]: readingNpcDialog,
  [ACT_END_PEACH_CUTSCENE]: endPeachCutscene,
  [ACT_JUMBO_STAR_CUTSCENE]: jumboStarCutscene,
  [ACT_SPAWN_NO_SPIN_AIRBORNE]: m => {
    marioSetForwardVel(m, 0);
    if (m.stepAir() === AIR_STEP_LANDED) m.setAction(ACT_SPAWN_NO_SPIN_LANDING);
    return false;
  },
  [ACT_SPAWN_NO_SPIN_LANDING]: warpLanding,
  [ACT_SPAWN_SPIN_AIRBORNE]: spawnSpinAirborne,
  [ACT_SPAWN_SPIN_LANDING]: warpLanding,
  [ACT_SPECIAL_EXIT_AIRBORNE]: starExitAirborne,
  [ACT_EXIT_AIRBORNE]: starExitAirborne,
  [ACT_FALLING_EXIT_AIRBORNE]: starExitAirborne,
  [ACT_EXIT_LAND_SAVE_DIALOG]: warpLanding,
  [ACT_STANDING_DEATH]: actStandingDeath,
  [ACT_TELEPORT_FADE_OUT]: m => {
    m.actionTimer++;
    stopAndSetHeightToFloor(m);
    return false;
  },
  [ACT_TELEPORT_FADE_IN]: m => {
    if (m.actionTimer++ === 32) {
      m.setAction(m.pos[1]<m.world.waterLevel(m.pos[0],m.pos[2])-100 ? ACT_WATER_IDLE : ACT_IDLE);
    }
    stopAndSetHeightToFloor(m);
    return false;
  },
  [ACT_DEATH_ON_BACK]: actDeathOnBack,
  [ACT_DEATH_ON_STOMACH]: actDeathOnStomach,
  [ACT_QUICKSAND_DEATH]: actQuicksandDeath,
};
