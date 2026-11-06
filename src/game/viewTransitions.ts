import type { CollisionWorld } from '../sim/collision';
import type { Vec3 } from '../sim/math';

export interface SidePlacement {
  pos: Vec3;
  laneZ: number;
  recovered: boolean;
}

/**
 * Pick a side-scrolling lane that has ground under Mario and ahead of him.
 * Falls back to nearby X/Z offsets so switching near a wall, ledge or while
 * airborne cannot leave the constrained lane inside geometry.
 */
export function findSidePlacement(world: CollisionWorld, pos: Vec3, probeForward = 220): SidePlacement {
  const offsets = [0, 120, -120, 240, -240, 360, -360, 480, -480];
  const baseY = pos[1];
  for (const dx of offsets) {
    const x = pos[0] + dx;
    for (const dz of offsets) {
      const z = pos[2] + dz;
      const floor = world.findFloor(x, baseY + 300, z);
      if (!floor.surface) continue;
      const ahead = world.findFloor(x + probeForward, baseY + 300, z);
      if (!ahead.surface) continue;
      if (Math.abs(floor.height - ahead.height) >= 260) continue;
      return {
        pos: [x, Math.max(baseY, floor.height + 2), z],
        laneZ: z,
        recovered: dx !== 0 || dz !== 0,
      };
    }
  }
  return { pos: [...pos] as Vec3, laneZ: pos[2], recovered: false };
}
