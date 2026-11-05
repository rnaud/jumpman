import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import {
  HELD_HELD,
  hitboxesOverlap,
  marioOverlapsObject,
  SimObject,
  type ObjectHitbox,
  type ObjectSpec,
} from '../src/sim/object';
import { initBobomb } from '../src/sim/behaviors/bobomb';
import type { ObjectEvent } from '../src/sim/objectManager';
import { ACT_HOLD_IDLE, ACT_PUNCHING, B_BUTTON, emptyController } from '../src/sim/state';
import { addGround } from './helpers';

function boxHitbox(): ObjectHitbox {
  return {
    interactType: 0,
    downOffset: 0,
    damageOrCoinValue: 0,
    health: 1,
    numLootCoins: 0,
    radius: 65,
    height: 113,
    hurtboxRadius: 0,
    hurtboxHeight: 0,
  };
}

function tick(mario: MarioRuntime, manager: ObjectManager, pressedB = false) {
  const controller = pressedB
    ? { ...emptyController(), buttonDown: B_BUTTON, buttonPressed: B_BUTTON }
    : emptyController();
  manager.step(mario);
  mario.step(controller, 0);
  manager.afterMarioStep(mario);
  return manager.drainEvents();
}

describe('SM64 object interactions', () => {
  it('tests hitbox overlaps in both axes', () => {
    const a = new SimObject({ behavior: 'bobomb', pos: [0, 0, 0], yaw: 0 });
    const b = new SimObject({ behavior: 'bobomb', pos: [100, 0, 0], yaw: 0 });
    a.setHitbox(boxHitbox());
    b.setHitbox(boxHitbox());
    expect(hitboxesOverlap(a, b)).toBe(true);
    b.pos[0] = 131;
    expect(hitboxesOverlap(a, b)).toBe(false);
    b.pos[0] = 0;
    b.pos[1] = 200;
    expect(hitboxesOverlap(a, b)).toBe(false);
  });

  it('drops a Bob-omb onto the collision floor with object_step gravity', () => {
    const world = new CollisionWorld();
    addGround(world);
    const object = new SimObject({ behavior: 'bobomb', pos: [0, 500, 0], yaw: 0 });
    initBobomb(object);
    for (let i = 0; i < 200; i++) object.stepPhysics(world);
    expect(object.pos[1]).toBeCloseTo(0, 5);
    expect(object.grounded).toBe(true);
    expect(object.vel[1]).toBe(0);
  });

  it('punches, picks up, carries and throws a Bob-omb', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'bobomb', pos: [0, 0, 80], yaw: 0 },
    ]);

    tick(mario, manager, true);
    expect(mario.action).toBe(ACT_PUNCHING);

    for (let i = 0; i < 24; i++) tick(mario, manager);
    expect(mario.heldObj).not.toBeNull();
    expect(mario.heldObj?.heldState).toBe(HELD_HELD);

    for (let i = 0; i < 24; i++) tick(mario, manager);
    expect(mario.action).toBe(ACT_HOLD_IDLE);

    tick(mario, manager, true);
    expect(mario.heldObj).not.toBeNull();
    for (let i = 0; i < 10; i++) tick(mario, manager);
    expect(mario.heldObj).toBeNull();

    let events: ObjectEvent[] = [];
    for (let i = 0; i < 90; i++) events = events.concat(tick(mario, manager));
    expect(events.some(event => event.type === 'explosion')).toBe(true);
    expect(events.some(event => event.type === 'coin')).toBe(true);
  });

  it('breaks a crate with a punch and hides the object', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'breakableBox', pos: [0, 0, 80], yaw: 0, param: 1 },
    ]);

    const events: ObjectEvent[] = [];
    tick(mario, manager, true);
    for (let i = 0; i < 8; i++) events.push(...tick(mario, manager));
    expect(events.some(event => event.type === 'break')).toBe(true);
    expect(manager.objects[0].active).toBe(false);
  });

  it('uses Mario and object hitboxes for proximity', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const object = new SimObject({ behavior: 'bobomb', pos: [0, 0, 80], yaw: 0 });
    initBobomb(object);
    expect(marioOverlapsObject(mario, object)).toBe(true);
    object.pos[2] = 500;
    expect(marioOverlapsObject(mario, object)).toBe(false);
  });

  it('spawns the Chain Chomp post and chain, releases it from a pounded post, and breaks the gate', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'chainChomp', pos: [0, 0, 300], yaw: 0, model: 'actors/chain_chomp/chain_chomp_geo.glb' },
      { behavior: 'chainChompGate', pos: [500, 0, 300], yaw: 0, model: 'levels/bob/bob_geo_000440.glb' },
    ]);

    manager.step(mario);
    const chomp = manager.objects.find(object => object.behavior === 'chainChomp')!;
    const gate = manager.objects.find(object => object.behavior === 'chainChompGate')!;
    expect(chomp.visible).toBe(true);
    expect(chomp.links).toHaveLength(6);

    const post = chomp.links![0];
    expect(post.behavior).toBe('woodenPost');
    post.vars.pounded = 1;
    post.vars.speedY = -120;
    const events: ObjectEvent[] = [];
    for (let i = 0; i < 20; i++) {
      manager.step(mario);
      events.push(...manager.drainEvents());
    }
    expect(chomp.vars.release).toBe(1);
    for (let i = 0; i < 220; i++) events.push(...tick(mario, manager));
    expect(events.some(event => event.type === 'break')).toBe(true);
    expect(gate.active).toBe(false);
  });

  it('damages Mario when he touches an active Chain Chomp', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'chainChomp', pos: [0, 0, 280], yaw: 0, model: 'actors/chain_chomp/chain_chomp_geo.glb' },
    ]);
    manager.step(mario);
    const chomp = manager.objects[0];
    chomp.pos = [0, 0, 80];
    manager.step(mario);
    expect(mario.hurtCounter).toBeGreaterThan(0);
    expect(mario.action).toBe(0x00020462);
  });

  it('is not left holding after a respawn', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const spec: ObjectSpec = { behavior: 'bobomb', pos: [0, 0, 80], yaw: 0 };
    const manager = new ObjectManager(world, [spec]);
    tick(mario, manager, true);
    for (let i = 0; i < 24; i++) tick(mario, manager);
    expect(mario.heldObj).not.toBeNull();
    mario.respawn();
    expect(mario.heldObj).toBeNull();
  });
});
