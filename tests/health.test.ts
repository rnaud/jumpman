import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import {
  ACT_SOFT_FORWARD_GROUND_KB,
  ACT_HARD_BACKWARD_GROUND_KB,
  ACT_HARD_FORWARD_GROUND_KB,
  ACT_DEATH_ON_BACK,
  ACT_LAVA_BOOST,
  ACT_QUICKSAND_DEATH,
  ACT_DROWNING,
  ACT_SOFT_BACKWARD_GROUND_KB,
  emptyController,
} from '../src/sim/state';
import { SURFACE_BURNING, SURFACE_DEEP_QUICKSAND } from '../src/sim/surfaces';
import { addGround } from './helpers';

function mario() {
  const world = new CollisionWorld();
  addGround(world);
  return new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
}

describe('health and damage', () => {
  it('drains one wedge over four hurt-counter frames', () => {
    const m = mario();
    expect(m.takeDamage(1, [0, 0, 100])).toBe(true);
    expect(m.action).toBe(ACT_SOFT_BACKWARD_GROUND_KB);
    expect(m.hurtCounter).toBe(4);
    for (let i = 0; i < 4; i++) m.step(emptyController(), 0);
    expect(m.health).toBe(0x880 - 0x100);
    expect(m.hurtCounter).toBe(0);
  });

  it('uses the source position to choose backward and forward knockback', () => {
    const back = mario();
    back.takeDamage(1, [0, 0, 100]);
    expect(back.action).toBe(ACT_SOFT_BACKWARD_GROUND_KB);

    const front = mario();
    front.takeDamage(1, [0, 0, -100]);
    expect(front.action).toBe(ACT_SOFT_FORWARD_GROUND_KB);
  });

  it('escalates knockback for heavy damage and low health', () => {
    const heavy = mario();
    heavy.takeDamage(4, [0, 0, 100]);
    expect(heavy.action).toBe(ACT_HARD_BACKWARD_GROUND_KB);

    const forward = mario();
    forward.takeDamage(4, [0, 0, -100]);
    expect(forward.action).toBe(ACT_HARD_FORWARD_GROUND_KB);

    const weak = mario();
    weak.health = 0x180;
    weak.takeDamage(1, [0, 0, 100]);
    expect(weak.action).toBe(ACT_HARD_BACKWARD_GROUND_KB);
  });

  it('ignores damage while the knockback action is invulnerable', () => {
    const m = mario();
    expect(m.takeDamage(1, [0, 0, 100])).toBe(true);
    expect(m.takeDamage(1, [0, 0, 100])).toBe(false);
    expect(m.hurtCounter).toBe(4);
  });

  it('heals one wedge over four heal-counter frames and caps at full health', () => {
    const m = mario();
    m.health = 0x100;
    m.healCounter = 4;
    for (let i = 0; i < 4; i++) m.step(emptyController(), 0);
    expect(m.health).toBe(0x200);

    m.health = 0x870;
    m.healCounter = 20;
    m.step(emptyController(), 0);
    expect(m.health).toBe(0x880);
  });

  it('drowns instead of playing a ground death when the last wedge is lost underwater', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, -700);
    world.waterBoxes = [[0, -500, -500, 500, 500, 0]];
    const m = new MarioRuntime(world, { pos: [0, -700, 0], faceYaw: 0 });
    m.step(emptyController(), 0);
    m.health = 0x100;
    m.hurtCounter = 4;
    m.step(emptyController(), 0);
    expect(m.action).toBe(ACT_DROWNING);

    for (let i = 0; i < 120; i++) m.step(emptyController(), 0);
    expect(m.respawns).toBe(1);
    expect(m.health).toBeGreaterThan(0x800);
  });

  it('starts a lava boost with the source burn counter', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0, SURFACE_BURNING);
    const m = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    m.step(emptyController(), 0);
    expect(m.action).toBe(ACT_LAVA_BOOST);
    expect(m.hurtCounter).toBeGreaterThanOrEqual(17);
    expect(m.vel[1]).toBeGreaterThan(0);
  });

  it('sinks through deep quicksand and respawns', () => {
    const world = new CollisionWorld();
    addGround(world, 4000, 0, SURFACE_DEEP_QUICKSAND);
    const m = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    let sawDeath = false;
    for (let i = 0; i < 400; i++) {
      m.step(emptyController(), 0);
      if (m.action === ACT_QUICKSAND_DEATH) { sawDeath = true; break; }
    }
    expect(sawDeath).toBe(true);
    for (let i = 0; i < 10; i++) m.step(emptyController(), 0);
    expect(m.respawns).toBeGreaterThan(0);
  });

  it('plays the death-on-back sequence before respawning on the last wedge', () => {
    const m = mario();
    m.health = 0x100;
    m.takeDamage(1, [0, 0, 100]);
    m.step(emptyController(), 0);
    expect(m.action).toBe(ACT_DEATH_ON_BACK);
    expect(m.events.some(event => event.type === 'death')).toBe(true);
    for (let i = 0; i < 60; i++) m.step(emptyController(), 0);
    expect(m.respawns).toBe(1);
    expect(m.health).toBe(0x880);
    expect(m.action).toBe(0x0c400201);
  });
});
