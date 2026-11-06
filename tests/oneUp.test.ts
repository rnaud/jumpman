import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { addGround, Sim } from './helpers';

describe('1-Up mushroom', () => {
  it('awards a life and emits a pickup event', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const sim = new Sim(world);
    sim.mario.health = 0x100;
    const manager = new ObjectManager(world, [{ behavior: 'oneUp', pos: [0, 40, 0], yaw: 0 }]);
    manager.step(sim.mario);
    expect(sim.mario.health).toBe(0x100);
    expect(sim.mario.lives).toBe(5);
    expect(manager.drainEvents().some(event => event.type === 'oneUp')).toBe(true);
  });
});
