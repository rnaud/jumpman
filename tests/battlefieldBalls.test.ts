import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { addNativeCollision, type CollisionData } from '../src/game/nativeCollision';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ObjectManager } from '../src/sim/objectManager';
import type { Vec3 } from '../src/sim/math';

const course: CollisionData & {
  mountainBallPaths: Vec3[][];
  mountainBallSpawners: {pos: Vec3; path: number; period: number; acts: number[]}[];
} = JSON.parse(readFileSync(new URL('../assets/courses/bob.json', import.meta.url), 'utf8'));

describe('Battlefield mountain bowling balls', () => {
  it('checks height and both distance limits before rolling the spawn odds', () => {
    const world = new CollisionWorld();
    addNativeCollision(world,course);
    world.finalize();
    const spec = course.mountainBallSpawners[0];
    const [x,y,z] = spec.pos;
    const mario = new MarioRuntime(world,{pos:[x,y,z],faceYaw:0});
    const route = {waypoints:course.mountainBallPaths[spec.path], period:128, maxDistance:7000};
    const manager = new ObjectManager(world,[{behavior:'mountainBallSpawner', pos:spec.pos, yaw:0, ballRoute:route}]);
    const rng = vi.spyOn(Math,'random').mockReturnValue(0.6);
    try {
      for (const pos of [[x,y,z+999], [x,y+1,z+1500], [x,y,z+7000]]) {
        mario.pos = pos as Vec3;
        for (let i=0; i<128; i++) manager.step(mario);
      }
      expect(rng).not.toHaveBeenCalled();
      mario.pos = [x,y,z+1000];
      for (let i=0; i<128; i++) manager.step(mario);
      expect(rng).toHaveBeenCalledTimes(1);
      expect(manager.findByBehavior('mountainBall')).toBeUndefined();
      rng.mockReturnValue(0.4);
      manager.step(mario);
      expect(manager.findByBehavior('mountainBall')?.forwardVel).toBe(20);
    } finally {rng.mockRestore();}
  });

  it.each([1,3])('rolls both paths to their final waypoint in act %i', act => {
    const world = new CollisionWorld();
    addNativeCollision(world,course);
    world.finalize();
    const mario = new MarioRuntime(world,{pos:[0,1024,-3100],faceYaw:0});
    const spawners = course.mountainBallSpawners.filter(s => s.acts.includes(act));
    expect(spawners.map(s => s.period)).toEqual([act < 3 ? 128 : 64,act < 3 ? 128 : 64]);
    const manager = new ObjectManager(world,spawners.map(s => ({
      pos:s.pos, yaw:0, behavior:'mountainBallSpawner',
      ballRoute:{waypoints:course.mountainBallPaths[s.path], period:s.period, maxDistance:s.path === 0 ? 7000 : 6000},
    })));
    const rng = vi.spyOn(Math,'random').mockReturnValue(0);
    try {
      manager.step(mario);
      const balls = manager.objects.filter(o => o.behavior === 'mountainBall');
      expect(balls).toHaveLength(2);
      rng.mockReturnValue(0.9);
      for (let i = 0; i < 1800 && balls.some(b => b.active); i++) manager.step(mario);
      expect(balls.map(b => [b.active,b.vars.waypoint])).toEqual(course.mountainBallPaths.map(path => [false,path.length-1]));
    } finally {rng.mockRestore();}
  });
});
