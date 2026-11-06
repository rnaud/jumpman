import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import {
  ACT_METAL_WATER_FALLING,
  ACT_METAL_WATER_STANDING,
  ACT_METAL_WATER_WALKING,
} from '../src/sim/state';
import { addGround, Sim } from './helpers';

describe('metal cap water mode', () => {
  it('sinks to the bottom and walks without drowning', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, -200);
    world.waterBoxes.push([1, -1000, -1000, 1000, 1000, 100]);
    const sim = new Sim(world);
    sim.mario.pos = [0, 0, 0];
    sim.mario.grantMetalCap(300);
    sim.tick({});
    expect([ACT_METAL_WATER_FALLING, ACT_METAL_WATER_STANDING, ACT_METAL_WATER_WALKING]).toContain(sim.mario.action);
    for (let i = 0; i < 120; i++) sim.tick({ y: -1 });
    expect(sim.mario.health).toBeGreaterThan(0x100);
    expect([ACT_METAL_WATER_STANDING, ACT_METAL_WATER_WALKING]).toContain(sim.mario.action);
    expect(sim.mario.pos[1]).toBeCloseTo(-200, 1);
  });
});
