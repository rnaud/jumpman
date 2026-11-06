import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_GRABBED } from '../src/sim/state';
import type { SimObject } from '../src/sim/object';
import { addGround, Sim } from './helpers';

describe('King Bob-omb grab', () => {
  it('holds Mario, then throws him with damage', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const king = {
      active: true,
      pos: [0, 0, 0],
      moveYaw: 0,
      hitbox: { height: 100 },
      vars: {},
    } as unknown as SimObject;
    expect(sim.mario.startKingGrab(king)).toBe(true);
    expect(sim.mario.action).toBe(ACT_GRABBED);
    sim.tick({});
    expect(sim.mario.grabbedBy).toBe(king);
    const health = sim.mario.health;
    sim.mario.throwFromGrab(king);
    expect(sim.mario.grabbedBy).toBeNull();
    sim.tick({});
    expect(sim.mario.health).toBeLessThan(health);
    expect(sim.mario.action & 0x1c0).toBe(0x80);
  });
});
