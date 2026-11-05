import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import {
  HELD_HELD,
  HELD_THROWN,
  hitboxesOverlap,
  marioOverlapsObject,
  SimObject,
  type ObjectHitbox,
  type ObjectSpec,
} from '../src/sim/object';
import { initBobomb } from '../src/sim/behaviors/bobomb';
import type { ObjectEvent } from '../src/sim/objectManager';
import {
  ACT_HOLDING_POLE,
  ACT_HOLD_IDLE,
  ACT_JUMP,
  ACT_PUNCHING,
  MARIO_PUNCHING,
  ACT_TOP_OF_POLE,
  B_BUTTON,
  emptyController,
} from '../src/sim/state';
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

  it('presses the purple switch, opens the grill and removes its barrier', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'purpleSwitch', pos: [0, 0, 100], yaw: 0, model: 'actors/purple_switch/purple_switch_geo.glb' },
      { behavior: 'openableGrill', pos: [0, 0, 300], yaw: 0, model: 'levels/bob/bob_geo_000470.glb' },
    ]);
    const groundSurfaces = world.surfaces.length;
    for (let i = 0; i < 6; i++) manager.step(mario);
    const purple = manager.objects.find(object => object.behavior === 'purpleSwitch')!;
    const grill = manager.objects.find(object => object.behavior === 'openableGrill')!;
    expect(purple.vars.action).toBe(2);
    expect(grill.vars.open).toBe(1);
    expect(world.surfaces.length).toBe(groundSurfaces);
  });

  it('rolls a pit bowling ball down a flat floor and damages Mario on contact', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'pitBowlingBall', pos: [0, 200, 100], yaw: 0, model: 'actors/bowling_ball/bowling_ball_geo.glb' },
    ]);
    for (let i = 0; i < 40; i++) manager.step(mario);
    const ball = manager.objects[0];
    expect(ball.pos[1]).toBeLessThan(20);
    expect(ball.pos[2]).toBeGreaterThan(100);

    ball.pos = [0, 0, 80];
    for (let i = 0; i < 2; i++) manager.step(mario);
    expect(mario.hurtCounter).toBeGreaterThan(0);
  });

  it('defeats King Bob-omb after three throws', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'kingBobomb', pos: [0, 0, 400], yaw: 0, model: 'actors/king_bobomb/king_bobomb_geo.glb' },
    ]);
    const king = manager.objects[0];
    const events: ObjectEvent[] = [];

    for (let throwIndex = 0; throwIndex < 3; throwIndex++) {
      king.heldState = HELD_THROWN;
      king.pos = [0, 120, 400];
      king.vel = [0, 0, 0];
      for (let i = 0; i < 60; i++) {
        manager.step(mario);
        events.push(...manager.drainEvents());
        if (king.vars.thrown === 0) break;
      }
    }

    expect(events.some(event => event.type === 'boss')).toBe(true);
    expect(king.active).toBe(false);
  });

  it('starts and wins the Koopa the Quick race at the flag', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [-4004, 0, 5400], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'koopaTheQuick', pos: [-4004, 0, 5221], yaw: 0, model: 'actors/koopa/koopa_with_shell_geo.glb' },
      { behavior: 'koopaFlag', pos: [-4004, 0, 5600], yaw: 0, model: 'actors/koopa_flag/koopa_flag_geo.glb' },
    ]);
    const events: ObjectEvent[] = [];
    for (let i = 0; i < 65; i++) {
      manager.step(mario);
      events.push(...manager.drainEvents());
    }
    expect(events.some(event => event.type === 'race' && event.state === 'start')).toBe(true);
    expect(events.some(event => event.type === 'race' && event.state === 'won')).toBe(true);
  });

  it('grabs a tree pole and climbs to the top', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 100, 200], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'tree', pos: [0, 0, 200], yaw: 0, model: 'actors/tree/bubbly_tree_geo.glb' },
    ]);
    mario.setAction(ACT_JUMP, 0);
    manager.step(mario);
    expect(mario.action).toBe(0x00100341);

    for (let i = 0; i < 20; i++) mario.step(emptyController(), 0);
    expect(mario.action).toBe(ACT_HOLDING_POLE);

    const climb = { ...emptyController(), stickY: 64, stickMag: 64 };
    for (let i = 0; i < 80; i++) {
      mario.step(climb, 0);
      if (mario.action === ACT_TOP_OF_POLE) break;
    }
    expect(mario.action).toBe(ACT_TOP_OF_POLE);
    expect(mario.pos[1]).toBeGreaterThan(300);
  });

  it('fires an exploding water bomb from the cannon', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'waterBombCannon', pos: [0, 0, 400], yaw: 0, model: 'actors/cannon_base/cannon_base_geo.glb' },
    ]);
    const events: ObjectEvent[] = [];
    let bombSeen = false;
    for (let i = 0; i < 200; i++) {
      manager.step(mario);
      events.push(...manager.drainEvents());
      if (manager.objects.some(object => object.behavior === 'waterBomb' && object.active)) bombSeen = true;
    }
    expect(bombSeen).toBe(true);
    expect(events.some(event => event.type === 'explosion')).toBe(true);
  });

  it('turns a punched Koopa into a kickable shell', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'koopa', pos: [0, 0, 80], yaw: 0, model: 'actors/koopa/koopa_with_shell_geo.glb' },
    ]);
    const koopa = manager.objects[0];
    mario.setAction(ACT_PUNCHING, 0);
    mario.flags |= MARIO_PUNCHING;
    manager.step(mario);

    expect(koopa.active).toBe(false);
    const shell = manager.objects.find(object => object.behavior === 'koopaShell');
    expect(shell?.active).toBe(true);

    expect(shell).toBeDefined();
    if (!shell) return;
    shell.pos = [0, 0, 60];
    mario.flags |= MARIO_PUNCHING;
    manager.step(mario);
    expect(shell.vars.sliding).toBe(1);
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
