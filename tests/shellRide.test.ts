import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ACT_RIDING_SHELL_GROUND, ACT_RIDING_SHELL_JUMP } from '../src/sim/state';
import type { SimObject } from '../src/sim/object';
import { addGround, Sim } from './helpers';

function flat(): Sim {
  const world = new CollisionWorld();
  addGround(world, 4000, 0);
  return new Sim(world);
}

describe('Koopa shell riding', () => {
  it('follows a shell and jumps off', () => {
    const sim = flat();
    const shell = {
      active: true,
      pos: [100, 0, 100],
      hitbox: { height: 50 },
      moveYaw: 0,
      forwardVel: 12,
      vars: {},
    } as unknown as SimObject;
    sim.mario.rideShell(shell);
    sim.tick({});
    expect(sim.mario.action).toBe(ACT_RIDING_SHELL_GROUND);
    expect(sim.mario.pos[0]).toBeCloseTo(100, 0);
    expect(sim.mario.pos[1]).toBeGreaterThan(shell.pos[1] + 50);
    sim.tick({ a: true, aPressed: true });
    expect(sim.mario.action).toBe(ACT_RIDING_SHELL_JUMP);
    expect(sim.mario.ridingShell).toBeNull();
  });
});
