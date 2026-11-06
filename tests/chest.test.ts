import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { B_BUTTON } from '../src/sim/state';
import { addGround, Sim } from './helpers';

describe('JRB treasure chest', () => {
  it('opens with B and emits an event', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'chest', pos: [0, 0, 0], yaw: 0 }]);
    sim.mario.controller = { stickX: 0, stickY: 0, stickMag: 0, buttonDown: B_BUTTON, buttonPressed: B_BUTTON };
    manager.step(sim.mario);
    expect(manager.objects[0].vars.opened).toBe(1);
    expect(manager.drainEvents().some(event => event.type === 'chest')).toBe(true);
  });
});
