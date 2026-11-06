import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { findWalkPath, marioRoute, segmentWalkable } from '../src/game/experiments/crowdRoute';
import { addGround } from './helpers';

describe('crowd route planning', () => {
  it('finds a straight path on flat ground', () => {
    const world = new CollisionWorld();
    addGround(world, 8000);
    expect(segmentWalkable(world, [-1720, 0, 120], [-2720, 0, 1120])).toBe(true);
    expect(findWalkPath(world, [-1720, 0, 120], [-2720, 0, 1120], 1500)).not.toBeNull();
    expect(findWalkPath(world, [1000, 0, 120], [-2720, 0, -1120], 1500)).not.toBeNull();
  });

  it('detours around a slippery slope Mario would slide down', () => {
    const world = new CollisionWorld();
    addGround(world, 8000);
    // A slippery ramp straight ahead, and a gentle normal ramp off to the side.
    world.addQuad([-300, 0, 600], [-300, 400, 1200], [300, 400, 1200], [300, 0, 600], { type: 0x14 });
    world.addQuad([-300, 400, 1200], [-300, 400, 1800], [300, 400, 1800], [300, 400, 1200], { type: 0 });
    world.addQuad([300, 400, 1200], [300, 400, 1800], [1500, 0, 1800], [1500, 0, 1200], { type: 0 });
    world.finalize();
    expect(segmentWalkable(world, [0, 0, 0], [0, 400, 1500])).toBe(false);
    const route = marioRoute(world, [[0, 0, 0], [0, 400, 1500]]);
    expect(route.length).toBeGreaterThan(2);
    for (let i = 1; i < route.length; i++) expect(segmentWalkable(world, route[i - 1], route[i])).toBe(true);
  });
});
