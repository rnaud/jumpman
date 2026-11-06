import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { INT_STATUS_WAS_ATTACKED } from '../src/sim/object';
import { addGround, Sim } from './helpers';

describe('Pokey', () => {
  it('takes three attacks and drops a coin', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'pokey', pos: [0, 40, 900], yaw: 0 }]);
    const pokey = manager.objects[0];
    for (let i = 0; i < 3; i++) {
      pokey.interactStatus |= INT_STATUS_WAS_ATTACKED;
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(pokey.active).toBe(false);
    expect(manager.drainEvents().some(event => event.type === 'coin')).toBe(true);
  });
});
