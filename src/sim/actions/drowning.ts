/**
 * Drowning and water-death actions, ported from
 * reference/sm64/src/game/mario_actions_submerged.c. The source starts the
 * death warp at animation frame 30 of the second drowning clip and frame 35 of
 * the water-death clip; Jumpman respawns at those frames.
 */

import type { MarioRuntime } from '../MarioRuntime';
import { ACT_DROWNING, ACT_WATER_DEATH } from '../state';

export function actDrowning(m: MarioRuntime): boolean {
  m.forwardVel *= 0.9;
  if (m.actionState === 0) {
    m.animLength = 79;
    if (m.animFrame >= 79) {
      m.actionState = 1;
      m.animFrame = -1;
    }
  } else {
    m.animLength = 69;
    if (m.animFrame >= 30) {
      m.respawn();
    }
  }
  return false;
}

export function actWaterDeath(m: MarioRuntime): boolean {
  m.forwardVel *= 0.9;
  m.animLength = 35;
  if (m.animFrame >= 35) {
    m.respawn();
  }
  return false;
}

export const SUBMERGED_ACTIONS: Record<number, (m: MarioRuntime) => boolean> = {
  [ACT_DROWNING]: actDrowning,
  [ACT_WATER_DEATH]: actWaterDeath,
};
