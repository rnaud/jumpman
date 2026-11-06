import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('Boo', () => {
  it('drifts toward Mario and damages on contact', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'boo', pos: [0, 40, 1200], yaw: 0 }]);
    const startZ = manager.objects[0].pos[2];
    for (let i = 0; i < 30; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(Math.abs(manager.objects[0].pos[2])).toBeLessThan(Math.abs(startZ));
  });
});
