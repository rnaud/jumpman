import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { INT_STATUS_WAS_ATTACKED } from '../src/sim/object';
import { addGround, Sim } from './helpers';

describe('Big Boo', () => {
  it('takes three attacks and emits a boss event', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'bigBoo', pos: [0, 40, 1200], yaw: 0 }]);
    const boss = manager.objects[0];
    for (let i = 0; i < 3; i++) {
      boss.interactStatus |= INT_STATUS_WAS_ATTACKED;
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(boss.active).toBe(false);
    expect(manager.drainEvents().some(event => event.type === 'bigBoo')).toBe(true);
  });
});
