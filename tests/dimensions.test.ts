import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { emptyController } from '../src/sim/state';
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
    expect(mario.intendedYaw).toBe(0xc000);
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
