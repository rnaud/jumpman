import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import {
  ACT_BUTT_SLIDE,
  ACT_CROUCH_SLIDE,
  ACT_DIVE,
  ACT_DOUBLE_JUMP,
  ACT_DOUBLE_JUMP_LAND,
  ACT_JUMP_LAND,
  ACT_LONG_JUMP,
  ACT_STOMACH_SLIDE,
  ACT_TRIPLE_JUMP,
  ACT_WALKING,
} from '../src/sim/state';
import { addBox, addGround, addRamp, Sim } from './helpers';

function flat(): Sim {
  const world = new CollisionWorld();
  addGround(world, 8000, 0);
  return new Sim(world);
}

describe('walking', () => {
  it('accelerates to Mario 64 run speed (32)', () => {
    const sim = flat();
    sim.run({ y: 1 }, 120);
    expect(sim.mario.action).toBe(ACT_WALKING);
    expect(sim.mario.forwardVel).toBeGreaterThan(31);
    expect(sim.mario.forwardVel).toBeLessThan(33);
    expect(sim.mario.pos[2]).toBeGreaterThan(1500);
  });

  it('turns with the camera-relative stick', () => {
    const sim = flat();
    sim.run({ y: 1 }, 30);
    const z = sim.mario.pos[2];
    // Fixed camera at -Z looking toward +Z: screen-right is -X.
    sim.run({ x: 1 }, 30);
    expect(sim.mario.pos[0]).toBeLessThan(-100);
    expect(sim.mario.pos[2]).toBeGreaterThan(z - 100);
  });
});

describe('jumping', () => {
  it('single jump apex is about 242 units with A held', () => {
    const sim = flat();
    sim.tick({ a: true, aPressed: true });
    let maxY = sim.mario.pos[1];
    for (let i = 0; i < 60; i++) {
      sim.tick({ a: true });
      maxY = Math.max(maxY, sim.mario.pos[1]);
    }
    expect(maxY).toBeGreaterThan(235);
    expect(maxY).toBeLessThan(250);
  });

  it('tapping A gives a short hop', () => {
    const sim = flat();
    sim.tick({ a: true, aPressed: true });
    let maxY = sim.mario.pos[1];
    for (let i = 0; i < 40; i++) {
      sim.tick({});
      maxY = Math.max(maxY, sim.mario.pos[1]);
    }
    // One frame of upward momentum is already spent before A is released.
    expect(maxY).toBeLessThan(100);
  });

  it('chains a double jump by pressing A as Mario lands', () => {
    const sim = flat();
    sim.tick({ a: true, aPressed: true });
    let landed = false;
    for (let i = 0; i < 120; i++) {
      sim.tick({ a: true });
      if (sim.mario.action === ACT_JUMP_LAND) {
        landed = true;
        break;
      }
    }
    expect(landed).toBe(true);
    sim.tick({ a: true, aPressed: true });
    expect(sim.mario.action).toBe(ACT_DOUBLE_JUMP);
    expect(sim.mario.vel[1]).toBeGreaterThan(45);
  });

  it('sets triple jump velocity from a double-jump landing', () => {
    const sim = flat();
    sim.mario.forwardVel = 30;
    sim.mario.doubleJumpTimer = 5;
    sim.mario.action = ACT_JUMP_LAND;
    sim.mario.prevAction = ACT_JUMP_LAND;
    sim.mario.setJumpFromLanding();
    expect(sim.mario.action).toBe(ACT_DOUBLE_JUMP);

    sim.mario.action = ACT_DOUBLE_JUMP_LAND;
    sim.mario.prevAction = ACT_DOUBLE_JUMP_LAND;
    sim.mario.forwardVel = 30;
    sim.mario.doubleJumpTimer = 5;
    sim.mario.setJumpFromLanding();
    expect(sim.mario.action).toBe(ACT_TRIPLE_JUMP);
    expect(sim.mario.vel[1]).toBeCloseTo(69, 5);
  });

  it('long jumps from a crouch slide', () => {
    const sim = flat();
    sim.run({ y: 1 }, 90);
    expect(sim.mario.forwardVel).toBeGreaterThan(30);

    sim.tick({ z: true, zPressed: true, y: 1 });
    expect(sim.mario.action).toBe(ACT_CROUCH_SLIDE);

    sim.tick({ a: true, aPressed: true, z: true, y: 1 });
    expect(sim.mario.action).toBe(ACT_LONG_JUMP);
    // One frame of gravity is applied on the same tick the action starts.
    expect(sim.mario.vel[1]).toBeCloseTo(28, 5);
    expect(sim.mario.forwardVel).toBeGreaterThan(44);
  });

  it('dives when B is pressed at speed', () => {
    const sim = flat();
    sim.run({ y: 1 }, 90);
    sim.tick({ b: true, bPressed: true, y: 1 });
    expect(sim.mario.action).toBe(ACT_DIVE);
    expect(sim.mario.vel[1]).toBeCloseTo(16, 5);
  });
});

describe('walls and slopes', () => {
  it('stops at a wall instead of passing through it', () => {
    const world = new CollisionWorld();
    addGround(world, 8000, 0);
    addBox(world, 0, 0, 400, 600, 300, 100);
    const sim = new Sim(world);

    sim.run({ y: 1 }, 120);
    // Front face at z=350, collision radius 50.
    expect(sim.mario.pos[2]).toBeLessThan(310);
    expect(sim.mario.pos[2]).toBeGreaterThan(290);
  });

  it('slides down a 45 degree slippery slope', () => {
    const world = new CollisionWorld();
    addRamp(world, 0, 0, 400, 400, 400);
    const sim = new Sim(world, [0, 200, 0]);
    sim.mario.pos[1] = sim.mario.floorHeight;

    for (let i = 0; i < 3; i++) sim.tick({});
    expect([ACT_BUTT_SLIDE, ACT_STOMACH_SLIDE]).toContain(sim.mario.action);
    const startY = sim.mario.pos[1];
    for (let i = 0; i < 15; i++) sim.tick({});
    expect(sim.mario.pos[1]).toBeLessThan(startY);
  });
});
