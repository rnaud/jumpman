import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('red flame hazard', () => {
  it('damages Mario on contact', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'flame', pos: [0, 0, 0], yaw: 0 }]);
    const before = sim.mario.health;
    for (let i = 0; i < 4; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(sim.mario.health).toBeLessThan(before);
  });
});
