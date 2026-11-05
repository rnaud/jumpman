import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import {
  ACT_AIR_HIT_WALL,
  ACT_BACKFLIP,
  ACT_CROUCH_SLIDE,
  ACT_CROUCHING,
  ACT_GROUND_POUND,
  ACT_GROUND_POUND_LAND,
  ACT_JUMP,
  ACT_WALL_KICK_AIR,
} from '../src/sim/state';
import { addBox, addGround, Sim } from './helpers';

function flat(): Sim {
  const world = new CollisionWorld();
  addGround(world, 8000, 0);
  return new Sim(world);
}

describe('special jump actions', () => {
  it('backflips out of a crouch', () => {
    const sim = flat();
    sim.tick({ z: true, zPressed: true });
    for (let i = 0; i < 12; i++) sim.tick({ z: true });
    expect(sim.mario.action).toBe(ACT_CROUCHING);

    sim.tick({ z: true, a: true, aPressed: true });
    expect(sim.mario.action).toBe(ACT_BACKFLIP);
    expect(sim.mario.vel[1]).toBeGreaterThan(55);
  });

  it('ground pounds from the air and lands', () => {
    const sim = flat();
    sim.tick({ a: true, aPressed: true });
    for (let i = 0; i < 6; i++) sim.tick({ a: true });
    sim.tick({ z: true, zPressed: true });
    expect(sim.mario.action).toBe(ACT_GROUND_POUND);

    let landed = false;
    for (let i = 0; i < 120; i++) {
      sim.tick({});
      if (sim.mario.action === ACT_GROUND_POUND_LAND) {
        landed = true;
        break;
      }
    }
    expect(landed).toBe(true);
  });

  it('wall kicks off a wall when A is pressed after the bonk', () => {
    const world = new CollisionWorld();
    addGround(world, 8000, 0);
    addBox(world, 0, 0, 250, 600, 400, 100);
    const sim = new Sim(world);

    sim.mario.pos = [0, 240, 100];
    sim.mario.vel = [0, 0, 26];
    sim.mario.forwardVel = 26;
    sim.mario.faceAngle = [0, 0, 0];
    sim.mario.action = 0x0100088c; // ACT_FREEFALL
    sim.mario.floorHeight = 0;

    let bonked = false;
    for (let i = 0; i < 12; i++) {
      sim.tick({});
      if (sim.mario.action === ACT_AIR_HIT_WALL) {
        bonked = true;
        break;
      }
    }
    expect(bonked).toBe(true);

    sim.tick({ a: true, aPressed: true });
    expect(sim.mario.action).toBe(ACT_WALL_KICK_AIR);
    expect(sim.mario.vel[1]).toBeGreaterThan(45);
  });

  it('keeps momentum when crouch-sliding on ice', () => {
    const world = new CollisionWorld();
    addGround(world, 8000, 0);
    addBox(world, 0, 0, 0, 5000, 10, 5000, { type: 0x2e }); // SURFACE_ICE
    const sim = new Sim(world, [0, 20, -1000]);
    sim.mario.pos[1] = sim.mario.floorHeight;

    sim.run({ y: 1 }, 70);
    expect(sim.mario.floorClass()).toBe(0x13);
    expect(sim.mario.forwardVel).toBeGreaterThan(30);

    sim.tick({ z: true, zPressed: true, y: 1 });
    expect(sim.mario.action).toBe(ACT_CROUCH_SLIDE);

    for (let i = 0; i < 25; i++) sim.tick({ z: true });
    // Very slippery ice uses a 0.98 loss factor, so speed barely decays.
    expect(sim.mario.forwardVel).toBeGreaterThan(15);
  });
});

describe('jump chains and ledge grabs', () => {
  function pressAOnLanding(sim: Sim, action: number, maxFrames = 300): boolean {
    for (let i = 0; i < maxFrames; i++) {
      sim.tick({ y: 1 });
      if (sim.mario.action === action) {
        sim.tick({ a: true, aPressed: true, y: 1 });
        return true;
      }
    }
    return false;
  }

  it('chains jump -> double jump -> triple jump', () => {
    const sim = flat();
    sim.run({ y: 1 }, 90);
    expect(sim.mario.forwardVel).toBeGreaterThan(30);

    sim.tick({ a: true, aPressed: true, y: 1 });
    expect(sim.mario.action).toBe(ACT_JUMP);

    // Land and immediately double jump.
    expect(pressAOnLanding(sim, 0x04000470)).toBe(true); // ACT_JUMP_LAND
    expect(sim.mario.action).toBe(0x03000881); // ACT_DOUBLE_JUMP

    // Land again and immediately triple jump.
    expect(pressAOnLanding(sim, 0x04000472)).toBe(true); // ACT_DOUBLE_JUMP_LAND
    expect(sim.mario.action).toBe(0x01000882); // ACT_TRIPLE_JUMP
    expect(sim.mario.vel[1]).toBeGreaterThan(60);
  });

  it('side flips when A is pressed while turning around', () => {
    const sim = flat();
    sim.run({ y: 1 }, 70);
    expect(sim.mario.forwardVel).toBeGreaterThan(25);

    sim.tick({ y: -1 });
    expect(sim.mario.action).toBe(0x00000443); // ACT_TURNING_AROUND

    sim.tick({ a: true, aPressed: true, y: -1 });
    expect(sim.mario.action).toBe(0x01000887); // ACT_SIDE_FLIP
  });

  it('emits separate ground-pound start and landing events', () => {
    const world = new CollisionWorld();
    addGround(world, 8000);
    const sim = new Sim(world);

    sim.mario.events.length = 0;
    sim.mario.setAction(0x008008a9); // ACT_GROUND_POUND
    expect(sim.mario.events.map(event => event.type)).toContain('groundpound');

    sim.mario.events.length = 0;
    sim.mario.setAction(0x0080023c); // ACT_GROUND_POUND_LAND
    expect(sim.mario.events.map(event => event.type)).toContain('groundpoundland');

    sim.mario.events.length = 0;
    sim.mario.setAction(0x03000888); // ACT_LONG_JUMP
    expect(sim.mario.events.map(event => event.type)).toContain('longjump');
  });

  it('completes the slow ledge climb after 28 frames', () => {
    const world = new CollisionWorld();
    addGround(world, 8000);
    const sim = new Sim(world);
    sim.mario.action = 0x0000054c; // ACT_LEDGE_CLIMB_SLOW_1
    sim.mario.actionTimer = 0;
    sim.mario.faceAngle = [0, 0, 0];
    sim.mario.pos = [0, 0, 0];
    sim.mario.floorHeight = 0;

    for (let i = 0; i < 30; i++) sim.tick({});
    expect(sim.mario.action).toBe(0x0c400201);
    expect(sim.mario.pos[2]).toBeGreaterThan(10);
  });

  it('dozes off after idle head-turn cycles and wakes on input', () => {
    const world = new CollisionWorld();
    addGround(world, 8000);
    const sim = new Sim(world);
    let slept = false;
    for (let i = 0; i < 1200; i++) {
      sim.tick({});
      if (sim.mario.action === 0x0c000203) {
        slept = true;
        break;
      }
    }
    expect(slept).toBe(true);

    sim.tick({ aPressed: true, a: true });
    expect(sim.mario.action).toBe(0x0c000204);
    for (let i = 0; i < 24; i++) sim.tick({});
    expect(sim.mario.action).toBe(0x0c400201);
  });

  it('grabs and climbs a ledge', () => {
    const world = new CollisionWorld();
    addGround(world, 8000, 0);
    addBox(world, 0, 0, 300, 600, 300, 100);
    const sim = new Sim(world);

    sim.mario.pos = [0, 210, 160];
    sim.mario.vel = [0, -4, 18];
    sim.mario.forwardVel = 18;
    sim.mario.faceAngle = [0, 0, 0];
    sim.mario.action = 0x0100088c; // ACT_FREEFALL
    sim.mario.floorHeight = 0;

    let grabbed = false;
    for (let i = 0; i < 10; i++) {
      sim.tick({});
      if (sim.mario.action === 0x0800034b) {
        grabbed = true;
        break;
      }
    }
    expect(grabbed).toBe(true);

    // Without A, the ledge grab auto-climbs after its 10 frame hold.
    for (let i = 0; i < 30; i++) sim.tick({});
    expect(sim.mario.pos[1]).toBeGreaterThan(290);
  });
});
