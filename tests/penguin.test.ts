import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('penguin', () => {
  it('wanders along the ground', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'penguin', pos: [0, 0, 0], yaw: 0 }]);
    const before = [...manager.objects[0].pos];
    for (let i = 0; i < 30; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(manager.objects[0].pos[0] !== before[0] || manager.objects[0].pos[2] !== before[2]).toBe(true);
  });
});
