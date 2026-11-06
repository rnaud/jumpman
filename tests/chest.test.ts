import { describe, expect, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { ObjectManager } from '../src/sim/objectManager';
import { Sim, addGround } from './helpers';

describe('ordered treasure chests', () => {
  it('requires facing, shocks on mistakes, closes previous lids and rewards a completed sequence once', () => {
    const world = new CollisionWorld();
    addGround(world, 6000, -5119);
    const m = new Sim(world).mario;
    const manager = new ObjectManager(world, [1, 2, 3, 4].map(i => ({
      behavior: 'chest', pos: [i * 700, -5119, 0], yaw: 0, param: i,
    })));
    const approach = (i: number, yaw = -32768) => {
      m.pos = [i * 700, -5119, 100];
      m.faceAngle[1] = yaw;
      manager.step(m);
      return manager.drainEvents();
    };
    expect(approach(1, 0)).toEqual([]);
    expect(approach(1)).toContainEqual({ type: 'chest', number: 1 });
    expect(approach(3)).toContainEqual({ type: 'chest', number: 0 });
    expect(m.hurtCounter).toBeGreaterThan(0);
    expect(approach(3)).toEqual([]);
    approach(0);
    expect(manager.objects[0].vars.opened).toBe(0);
    for (const i of [1, 2, 3, 4]) expect(approach(i)).toContainEqual({ type: 'chest', number: i });
    for (let i = 0; i < 61; i++) expect(approach(0)).toEqual([]);
    expect(approach(0)).toContainEqual({ type: 'chestReward' });
    expect(approach(0)).toEqual([]);
    expect(manager.objects.every(o => o.active && o.vars.lidPitch === -0x4000)).toBe(true);
    manager.reset();
    expect(approach(1)).toContainEqual({ type: 'chest', number: 1 });
  });
});
