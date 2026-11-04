import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addBox, addGround, addRamp } from './helpers';

describe('triangle collision', () => {
  it('finds a flat floor', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    const hit = world.findFloor(10, 100, -20);
    expect(hit.surface).not.toBeNull();
    expect(hit.height).toBeCloseTo(0, 6);
  });

  it('finds the top of a box', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    addBox(world, 0, 0, 0, 200, 100, 200);
    const hit = world.findFloor(0, 200, 0);
    expect(hit.height).toBeCloseTo(100, 6);
  });

  it('computes a 45 degree ramp normal and height', () => {
    const world = new CollisionWorld();
    addRamp(world, 0, 0, 200, 200, 200);
    const hit = world.findFloor(0, 200, 0);
    expect(hit.surface?.normal[1]).toBeCloseTo(Math.SQRT1_2, 4);
    expect(hit.height).toBeCloseTo(100, -1);
  });

  it('pushes out of a wall by the collision radius', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    addBox(world, 0, 0, 100, 400, 300, 100);

    const pos: [number, number, number] = [0, 30, 20];
    world.f32FindWallCollision(pos, 0, 50);
    // The wall's front face is at z=50; a 50-unit radius means the centre is
    // pushed back to z=0.
    expect(pos[2]).toBeCloseTo(0, 5);
  });

  it('only sees floors within the 78 unit buffer', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0);
    expect(world.findFloor(0, -78, 0).surface).not.toBeNull();
    expect(world.findFloor(0, -79, 0).surface).toBeNull();
  });

  it('prefers the first (highest sorted) floor', () => {
    const world = new CollisionWorld();
    // Two overlapping floors; SM64 sorts floors highest-first by vertex1.y.
    world.addQuad([-100, 100, -100], [-100, 100, 100], [100, 100, 100], [100, 100, -100]);
    world.addQuad([-100, 0, -100], [-100, 0, 100], [100, 0, 100], [100, 0, -100]);
    world.finalize();
    expect(world.findFloor(0, 150, 0).height).toBeCloseTo(100, 6);
  });
});
