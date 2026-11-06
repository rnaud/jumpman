import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('poison gas hazard', () => {
  it('damages Mario while he stands inside the cloud', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'poisonGas', pos: [0, 0, 0], yaw: 0 }]);
    const health = sim.mario.health;
    for (let i = 0; i < 50; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(sim.mario.health).toBeLessThan(health);
  });
});
