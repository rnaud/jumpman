/**
 * Cutscene-group death actions, ported from
 * reference/sm64/src/game/mario_actions_cutscene.c. The source triggers a
 * level warp after a fixed animation frame; Jumpman respawns at that frame.
 */

import type { MarioRuntime } from '../MarioRuntime';
import {
  ACT_DEATH_ON_BACK,
  ACT_DEATH_ON_STOMACH,
  ACT_STANDING_DEATH,
} from '../state';
import { stopAndSetHeightToFloor } from '../marioStep';

function deathFrame(m: MarioRuntime, frames: number): boolean {
  stopAndSetHeightToFloor(m);
  m.animLength = frames;
  if (++m.actionTimer >= frames) {
    m.respawn();
  }
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

export const CUTSCENE_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_STANDING_DEATH]: actStandingDeath,
  [ACT_DEATH_ON_BACK]: actDeathOnBack,
  [ACT_DEATH_ON_STOMACH]: actDeathOnStomach,
};
