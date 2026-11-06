import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('Klepto', () => {
  it('circles its home in the air', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'klepto', pos: [3000, 800, 3000], yaw: 0 }]);
    const before = [...manager.objects[0].pos];
    for (let i = 0; i < 60; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(manager.objects[0].pos[0] !== before[0] || manager.objects[0].pos[2] !== before[2]).toBe(true);
  });
});
