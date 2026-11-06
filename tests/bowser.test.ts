import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { INT_STATUS_WAS_ATTACKED } from '../src/sim/object';
import { addGround, Sim } from './helpers';

describe('Bowser', () => {
  it('breathes fire projectiles', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'bowser', pos: [0, 40, 1800], yaw: 0 }]);
    for (let i = 0; i < 125; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(manager.objects.some(object => object.behavior === 'bowserFlame' && object.active)).toBe(true);
  });

  it('takes three attacks and emits a defeat event', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'bowser', pos: [0, 40, 1800], yaw: 0 }]);
    const boss = manager.objects[0];
    for (let i = 0; i < 3; i++) {
      boss.interactStatus |= INT_STATUS_WAS_ATTACKED;
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(boss.active).toBe(false);
    expect(manager.drainEvents().some(event => event.type === 'bowser')).toBe(true);
  });
});
