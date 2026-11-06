import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { DoorCollider } from '../src/game/roomDoors';
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
import { BOBOMB_ACT_CHASE_MARIO, BOBOMB_ACT_EXPLODE, initBobomb } from '../src/sim/behaviors/bobomb';
import type { ObjectEvent } from '../src/sim/objectManager';
import {
  ACT_GROUND_POUND_LAND, ACT_IDLE, ACT_WALKING, ACT_HOLDING_POLE,
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
    expect(manager.objects.filter(o => o.behavior === 'movingYellowCoin')).toHaveLength(1);
  });

  it('finishes a lit fuse, waits for Mario to leave home and pays once per Bob-omb', () => {
    const world = new CollisionWorld();
    addGround(world,6000);
    const mario = new MarioRuntime(world,{pos:[0,0,500],faceYaw:0});
    const manager = new ObjectManager(world,[{behavior:'bobomb',pos:[0,0,0],yaw:0}]);
    const bomb = manager.objects[0];
    bomb.setAction(BOBOMB_ACT_CHASE_MARIO);
    bomb.vars.fuseLit = 1;
    bomb.vars.fuseTimer = 151;
    const step = (count: number) => { for (let i=0;i<count;i++) manager.step(mario); };
    step(7);
    expect(bomb.active).toBe(false);
    const coins = () => manager.objects.filter(o=>o.behavior==='movingYellowCoin');
    expect(coins()).toHaveLength(1);
    mario.pos = [0,0,2999];
    step(400);
    expect(bomb.active).toBe(false);
    expect(coins()[0].active).toBe(false);
    mario.pos = [0,0,3000];
    step(1);
    expect(bomb.active).toBe(true);
    mario.pos = [0,0,500];
    bomb.setAction(BOBOMB_ACT_EXPLODE);
    step(6);
    expect(bomb.active).toBe(false);
    expect(coins()).toHaveLength(1);
    manager.reset();
    expect(manager.objects[0].coinDropped).toBe(false);
  });

  it('only respawns an intact released corkbox after the timeout and the distance check', () => {
    const world = new CollisionWorld();
    addGround(world,6000);
    const mario = new MarioRuntime(world,{pos:[0,0,600],faceYaw:0});
    const manager = new ObjectManager(world,[{behavior:'breakableBoxSmall',pos:[0,0,0],yaw:0}]);
    const box = manager.objects[0];
    box.heldState = HELD_THROWN;
    manager.step(mario);
    // Rest the released box on the floor away from walls.
    box.forwardVel = 0;
    for(let i=0;i<901;i++) manager.step(mario);
    expect(box.active).toBe(false);
    expect(manager.objects).toHaveLength(1);
    mario.pos = [3000,0,0];
    manager.step(mario);
    expect(box.active).toBe(true);
    expect(box.vars.released).toBe(0);
    expect(box.visible).toBe(true);
  });

  it('counts net laps around an untouched post within 200 frames and pays only once', () => {
    const world = new CollisionWorld();
    addGround(world);
    const mario = new MarioRuntime(world,{pos:[0,0,600],faceYaw:0});
    const manager = new ObjectManager(world,[{behavior:'woodenPost',pos:[0,0,0],yaw:0}]);
    function turn(frames: number, framesPerLap: number) {
      let coins = 0;
      for (let i=0;i<frames;i++) {
        const angle = i*2*Math.PI/framesPerLap;
        mario.pos = [150*Math.sin(angle),0,150*Math.cos(angle)];
        manager.step(mario);
        coins += manager.drainEvents().filter(e=>e.type==='coin').length;
      }
      return coins;
    }
    expect(turn(242,80)).toBe(0);
    mario.pos = [0,0,500];
    manager.step(mario);
    expect(turn(90,30)).toBe(0);
    // Reverse direction, cancelling turns already made.
    expect(turn(95,-30)).toBe(0);
    mario.pos = [0,0,500];
    manager.step(mario);
    expect(turn(110,-30)).toBe(5);
    expect(turn(110,30)).toBe(0);
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
    expect(chomp.links).toHaveLength(5);
    expect(chomp.links!.slice(1).every(link => link.billboard)).toBe(true);

    const post = chomp.links![0];
    expect(post.behavior).toBe('woodenPost');
    const top = new DoorCollider(world, {
      vertices: [[-40,205,-40], [-40,205,40], [40,205,40], [40,205,-40]],
      triangles: [[0,1,2,0,0], [0,2,3,0,0]],
    }, 0, post);
    // Landing at the right coordinates on a different surface must not pound the post.
    mario.pos = [post.pos[0], post.pos[1]+205, post.pos[2]];
    mario.floor = world.findFloor(...mario.pos).surface;
    mario.setAction(ACT_GROUND_POUND_LAND);
    manager.step(mario);
    expect(post.vars.pounded).toBe(0);
    for (let i = 1; i <= 3; i++) {
      top.move(post.pos);
      mario.pos = [post.pos[0], post.pos[1]+205, post.pos[2]];
      const hit = world.findFloor(...mario.pos);
      mario.floor = hit.surface;
      mario.floorHeight = hit.height;
      mario.setAction(ACT_GROUND_POUND_LAND);
      manager.step(mario);
      mario.setAction(ACT_IDLE);
      for (let frame=0; frame<4; frame++) manager.step(mario);
      expect(post.vars.offsetY).toBe(Math.max(-190,-65*i));
      expect(Boolean(chomp.vars.release)).toBe(i === 3);
    }
    // The gate listens for the Chomp's wall collision in its gate-breaking lunge.
    chomp.vars.release = 3;
    chomp.hitWall = true;
    const events: ObjectEvent[] = [];
    for (let i=0; i<3; i++) events.push(...tick(mario,manager));
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

  it.each([[false, 1], [true, -1]])('records a grounded invitation and a 3D race finish (cannon %s)', (cannon, result) => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, {pos:[-4004,0,5400],faceYaw:0});
    const manager = new ObjectManager(world, [
      {behavior:'koopaTheQuick',pos:[-4004,0,5221],yaw:0},
      {behavior:'koopaFlag',pos:[3304,4242,-4603],yaw:0},
    ]);
    const [koopa,flag] = manager.objects;
    manager.step(mario);
    expect(koopa.action).toBe(1);
    for (let i=0;i<65;i++) manager.step(mario);
    expect(koopa.action).toBe(1);
    koopa.vars.answer = 1;
    manager.step(mario);
    koopa.vars.answer = 0;
    for (let i=0;i<50;i++) manager.step(mario);
    expect(koopa.action).toBe(2);
    manager.step(mario);
    expect(manager.drainEvents()).toContainEqual({type:'race',state:'start'});
    if (cannon) mario.setAction(0x00880898);
    mario.pos = [3304,3300,-4603];
    manager.step(mario);
    expect(flag.vars.result).toBeFalsy();
    mario.pos = [...flag.pos];
    manager.step(mario);
    expect(flag.vars.result).toBe(result);
    expect(manager.drainEvents().some(e=>e.type==='race' && e.state==='won')).toBe(false);
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
    for (let i = 0; i < 60; i++) mario.step(climb, 0);
    mario.step(emptyController(), 0); // Release at the top, then push up for a handstand.
    for (let i = 0; i < 20; i++) {
      mario.step(climb, 0);
      if (mario.action === ACT_TOP_OF_POLE) break;
    }
    expect(mario.action).toBe(ACT_TOP_OF_POLE);
    expect(mario.pos[1]).toBeGreaterThan(300);
  });

  it('keeps the starting cannon quiet and fires harmless mountain shots only while in range', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior:'waterBombCannon',pos:[0,0,400],yaw:0,param:1 },
      { behavior:'waterBombCannon',pos:[5000,0,0],yaw:0,param:0 },
    ]);
    const sounds: number[] = [];
    let frame = 0;
    const step = (count: number) => {
      for (let i=0; i<count; i++,frame++) {
        manager.step(mario);
        for (const event of manager.drainEvents()) {
          expect(event.type).not.toBe('explosion');
          if (event.type === 'waterCannon') sounds.push(frame);
        }
      }
    };
    step(900);
    expect(sounds).toHaveLength(0);
    expect(manager.objects.some(o => o.behavior === 'waterBomb')).toBe(false);
    mario.pos = [5000,0,360];
    step(350);
    expect(sounds.length).toBeGreaterThanOrEqual(2);
    expect(sounds[1]-sounds[0]).toBeGreaterThanOrEqual(131);
    const bomb = manager.objects.find(o => o.behavior === 'waterBomb')!;
    expect(bomb.hitbox.interactType).toBe(0);
    const count = sounds.length;
    mario.pos = [0,0,0];
    step(350);
    expect(sounds).toHaveLength(count);
    expect(bomb.active).toBe(false);
  });

  it('gates predicted overhead drops by range and reuses one bomb and shadow after three bounces', () => {
    const world = new CollisionWorld(); addGround(world,6000);
    const mario = new MarioRuntime(world,{pos:[0,0,951],faceYaw:0});
    const manager = new ObjectManager(world,[{behavior:'waterBombSpawner',param:15,pos:[0,4000,0],yaw:0}]);
    manager.step(mario);
    expect(manager.objects).toHaveLength(1);
    mario.pos = [0,1000,0]; manager.step(mario);
    expect(manager.objects).toHaveLength(1);
    mario.pos = [0,0,0]; mario.forwardVel=10; manager.step(mario);
    const bomb = manager.findByBehavior('waterBomb')!;
    const shadow = manager.findByBehavior('waterBombShadow')!;
    expect(bomb.pos).toEqual([0,2000,380]);
    expect(shadow.pos[1]).toBe(500);
    mario.pos = [4000,0,0];
    const phases: string[] = [];
    for (let t=0;t<250;t++) {
      manager.step(mario);
      for (const event of manager.drainEvents()) if (event.type==='waterBomb') phases.push(event.phase);
    }
    expect(phases).toEqual(['bounce','bounce','splash','burst']);
    expect(bomb.active || shadow.active).toBe(false);
    mario.pos = [0,0,0]; mario.forwardVel=0;
    for (let t=0;t<60 && !bomb.active;t++) manager.step(mario);
    expect(manager.objects).toHaveLength(3);
    expect(bomb.active && shadow.active).toBe(true);
    mario.pos = [0,0,100];
    for (let t=0;t<65 && !mario.hurtCounter;t++) manager.step(mario);
    expect(mario.hurtCounter).toBe(4);
    manager.step(mario);
    expect(bomb.active).toBe(false);
  });

  it('bursts when an overhead bomb hits water before the floor', () => {
    const world = new CollisionWorld(); addGround(world);
    world.waterBoxes.push([0,-500,-500,500,500,700]);
    const mario = new MarioRuntime(world,{pos:[0,0,0],faceYaw:0});
    const manager = new ObjectManager(world,[{behavior:'waterBombSpawner',param:10,pos:[0,4000,0],yaw:0}]);
    manager.step(mario);
    mario.pos = [3000,0,0];
    const phases: string[] = [];
    for (let i=0;i<100;i++) {
      manager.step(mario);
      for (const event of manager.drainEvents()) if (event.type === 'waterBomb') phases.push(event.phase);
    }
    expect(phases).toEqual(['splash','burst']);
  });

  it('knocks the shell off a Koopa and lets Mario mount on foot', () => {
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

    expect(koopa.active && koopa.vars.unshelled).toBe(1);
    const shell = manager.objects.find(object => object.behavior === 'koopaShell');
    expect(shell?.active).toBe(true);

    expect(shell).toBeDefined();
    if (!shell) return;
    shell.pos = [0, 0, 60];
    mario.flags |= MARIO_PUNCHING;
    manager.step(mario);
    expect(mario.ridingShell).toBeNull();
    mario.setAction(ACT_WALKING);
    manager.step(mario);
    expect(mario.ridingShell).toBe(shell);
  });

  it('shows Bob-omb Buddy advice when Mario approaches', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'bobombBuddy', pos: [0, 0, 100], yaw: 0, model: 'actors/bobomb/bobomb_buddy_geo.glb' },
    ]);
    const events: ObjectEvent[] = [];
    for (let i = 0; i < 3; i++) {
      manager.step(mario);
      events.push(...manager.drainEvents());
    }
    expect(events.some(event => event.type === 'dialogue')).toBe(true);
  });

  it('emits a warp when Mario touches a warp trigger', () => {
    const world = new CollisionWorld();
    addGround(world, 6000);
    const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
    const manager = new ObjectManager(world, [
      { behavior: 'warp', pos: [0, 0, 80], yaw: 0, model: '', warpTo: 'pss' },
    ]);
    const events: ObjectEvent[] = [];
    for (let i = 0; i < 3; i++) {
      manager.step(mario);
      events.push(...manager.drainEvents());
    }
    expect(events.some(event => event.type === 'warp' && event.destination === 'pss')).toBe(true);
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
