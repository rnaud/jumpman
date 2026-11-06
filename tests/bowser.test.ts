import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { HELD_THROWN, INT_STATUS_WAS_ATTACKED } from '../src/sim/object';
import { ACT_HOLDING_BOWSER } from '../src/sim/state';
import { addGround, Sim, type TestInput } from './helpers';

describe('Bowser', () => {
  it('breathes fire projectiles', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, 0);
    const sim = new Sim(world);
    const manager = new ObjectManager(world, [{ behavior: 'bowser', pos: [0, 40, 1800], yaw: 0 }]);
    for (let i = 0; i < 125; i++) {
      manager.step(sim.mario);
      sim.tick({});
    }
    expect(manager.objects.some(object => object.behavior === 'bowserFlame' && object.active)).toBe(true);
  });

  it('grabs the tail with B, swings, releases into a mine and awards the first arena reward', () => {
    const world = new CollisionWorld();
    addGround(world, 10000);
    const sim = new Sim(world, [0, 0, -300]);
    const manager = new ObjectManager(world, [
      { behavior: 'bowser', param: 1, pos: [0, 0, 0], yaw: 0 },
      { behavior: 'bowserMine', pos: [0, 100, 2200], yaw: 0 },
    ]);
    const [boss, mine] = manager.objects;
    const tick = (input: TestInput = {}) => {
      manager.step(sim.mario);
      sim.tick(input);
      manager.afterMarioStep(sim.mario);
      return manager.drainEvents();
    };
    boss.interactStatus = INT_STATUS_WAS_ATTACKED;
    tick({b: true, bPressed: true});
    for (let i = 0; i < 24; i++) tick();
    expect(boss.vars.health).toBe(1);
    expect(sim.mario.heldObj).toBe(boss);
    expect(sim.mario.action).toBe(ACT_HOLDING_BOWSER);
    for (let i = 0; i < 160; i++) {
      tick({x: Math.sin(i * Math.PI / 2), y: Math.cos(i * Math.PI / 2)});
      if (Math.abs(sim.mario.angleVel[1]) === 4096 && Math.abs(sim.mario.faceAngle[1]) <= 2048) break;
    }
    expect(Math.abs(sim.mario.angleVel[1])).toBe(4096);
    tick({b: true, bPressed: true});
    expect(sim.mario.heldObj).toBeNull();
    let defeated = false;
    for (let i = 0; i < 200; i++) {
      defeated ||= tick().some(event => event.type === 'bowser');
    }
    expect(mine.active).toBe(false);
    expect(defeated).toBe(true);
  });

  it('uses stage-specific health and consumes each mine once', () => {
    const world = new CollisionWorld();
    addGround(world, 10000);
    const sim = new Sim(world, [0, 0, -1500]);
    const manager = new ObjectManager(world, [
      { behavior: 'bowser', param: 3, pos: [0, 40, 0], yaw: 0 },
      { behavior: 'bowserMine', pos: [0, 100, 600], yaw: 0 },
    ]);
    const [boss, mine] = manager.objects;
    expect(boss.vars.health).toBe(3);
    boss.heldState = HELD_THROWN;
    for (let i = 0; i < 100; i++) manager.step(sim.mario);
    expect(mine.active).toBe(false);
    expect(boss.vars.health).toBe(2);
    expect(boss.active).toBe(true);
  });
});
