import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { findSidePlacement } from '../src/game/viewTransitions';
import { addGround } from './helpers';

describe('side-view transition placement', () => {
  it('finds a nearby safe lane when the current lane has a gap', () => {
    const world = new CollisionWorld();
    world.addQuad([-600, 0, -1200], [-600, 0, -400], [600, 0, -400], [600, 0, -1200]);
    world.addQuad([-600, 0, 400], [-600, 0, 1200], [600, 0, 1200], [600, 0, 400]);
    world.finalize();
    const placement = findSidePlacement(world, [0, 0, 0]);
    expect(placement.pos.every(Number.isFinite)).toBe(true);
    expect(placement.laneZ).not.toBe(0);
    expect(placement.recovered).toBe(true);
  });

  it('keeps the current lane when it is already safe', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const placement = findSidePlacement(world, [0, 0, 0]);
    expect(placement.laneZ).toBe(0);
    expect(placement.recovered).toBe(false);
  });
});
