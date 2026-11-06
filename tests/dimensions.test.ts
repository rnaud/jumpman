import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { toS16 } from '../src/sim/math';
import { emptyController } from '../src/sim/state';
import { distanceForHeight, laneYawFromCamera, viewHeight } from '../src/game/experiments/dimensions';
import { addGround } from './helpers';

describe('Across Dimensions movement mapping', () => {
  it('maps horizontal stick input to world X in side view', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    mario.movementMode = 'side';
    mario.step({ ...emptyController(), stickX: 64, stickMag: 64 }, 0);
    expect(mario.intendedYaw).toBe(0x4000);
    mario.step({ ...emptyController(), stickX: -64, stickMag: 64 }, 0);
    expect(toS16(mario.intendedYaw)).toBe(toS16(0xc000));
  });

  it('runs along any lane direction in side view', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    mario.movementMode = 'side';
    mario.sideYaw = 0x2000;
    mario.step({ ...emptyController(), stickX: 64, stickMag: 64 }, 0);
    expect(mario.intendedYaw).toBe(0x2000);
    mario.step({ ...emptyController(), stickX: -64, stickMag: 64 }, 0);
    expect(mario.intendedYaw).toBe(toS16(0xa000));
  });

  it('snaps the side lane to the camera’s screen-right in 45° steps', () => {
    // Camera looking toward -Z (yaw 0x8000): screen-right is +X (yaw 0x4000).
    expect(laneYawFromCamera(0x8000 - 0x10000)).toBe(0x4000);
    expect(laneYawFromCamera(0x0000)).toBe(toS16(0xc000));
    expect(laneYawFromCamera(0x8000 + 0x0900 - 0x10000)).toBe(0x4000);
    expect(laneYawFromCamera(0x8000 + 0x1900 - 0x10000)).toBe(0x6000);
  });

  it('keeps the visible height constant through the flattening dolly zoom', () => {
    const distance = distanceForHeight(1500, 7);
    expect(viewHeight(distance, 7)).toBeCloseTo(1500, 6);
    expect(distance).toBeGreaterThan(10000);
  });

  it('maps screen-up to -Z in top-down view', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    mario.movementMode = 'topdown';
    mario.step({ ...emptyController(), stickY: 64, stickMag: 64 }, 0);
    expect(mario.intendedYaw).toBe(-0x8000);
    mario.step({ ...emptyController(), stickX: 64, stickMag: 64 }, 0);
    expect(mario.intendedYaw).toBe(0x4000);
  });
});
