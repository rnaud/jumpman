import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('Bully', () => {
  it('charges toward Mario and damages on contact', () => {
    const world = new CollisionWorld();
    addGround(world, 5000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'bully', pos: [0, 40, 1000], yaw: 0 }]);
    const before = sim.mario.health;
    for (let i = 0; i < 200; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(Math.abs(manager.objects[0].pos[2])).toBeLessThan(1000);
    expect(sim.mario.health).toBeLessThan(before);
  });
});
