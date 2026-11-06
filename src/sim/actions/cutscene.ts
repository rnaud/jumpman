/**
 * Cutscene-group death actions, ported from
 * reference/sm64/src/game/mario_actions_cutscene.c. The source triggers a
 * level warp after a fixed animation frame; Jumpman respawns at that frame.
 */

import { jumboStarCutscene, endPeachCutscene } from './ending';
import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_SPAWN_NO_SPIN_AIRBORNE, ACT_SPAWN_NO_SPIN_LANDING,
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

export const CUTSCENE_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
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
