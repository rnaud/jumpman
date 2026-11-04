import { existsSync, readFileSync } from 'node:fs';
import { describe, it, expect } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { emptyController } from '../src/sim/state';
import { addNativeCollision } from '../src/game/nativeCollision';
const available = existsSync('assets/courses/pss.json');
describe.skipIf(!available)('native course collision', () => {
  for (const id of ['bob', 'castle_grounds', 'wf', 'pss']) {
    it(`loads typed terrain and the original spawn for ${id}`, () => {
      const data = JSON.parse(readFileSync(`assets/courses/${id}.json`, 'utf8'));
      const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
      const [x,y,z] = data.spawn.pos;
      const floor = world.findFloor(x, y + 200, z);
      expect(floor.surface).not.toBeNull();
      expect(floor.surface?.type).not.toBe(10);
      expect(world.surfaces.some(s => s.type !== 0)).toBe(true);
    });
  }
  it('allows the bottom of the slide and respawns on its death plane', () => {
    const data = JSON.parse(readFileSync('assets/courses/pss.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    const floor = world.findFloor(-6385, -4000, 5770);
    expect(floor.height).toBeLessThan(-1600);
    const mario = new MarioRuntime(world, { pos: [-6385, floor.height, 5770], faceYaw: 0 }, -11000);
    for (let i = 0; i < 30; i++) mario.step(emptyController(), 0);
    expect(mario.respawns).toBe(0);
    const death = world.surfaces.find(s => s.type === 10)!;
    mario.pos = [0, 0, 0].map((_, i) => (death.v1[i] + death.v2[i] + death.v3[i]) / 3) as [number,number,number];
    mario.step(emptyController(), 0);
    expect(mario.respawns).toBe(1);
  });
});
