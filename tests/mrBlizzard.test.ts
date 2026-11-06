import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { INT_STATUS_WAS_ATTACKED } from '../src/sim/object';
import { addGround, Sim } from './helpers';

describe('Mr. Blizzard', () => {
  it('is defeated by an attack and drops a coin', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'mrBlizzard', pos: [0, 40, 600], yaw: 0 }]);
    const snowman = manager.objects[0];
    snowman.interactStatus |= INT_STATUS_WAS_ATTACKED;
    manager.step(sim.mario);
    expect(snowman.active).toBe(false);
    expect(manager.drainEvents().some(event => event.type === 'coin')).toBe(true);
  });
});
