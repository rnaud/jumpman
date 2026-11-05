import { existsSync, readFileSync } from 'node:fs';
import { describe, it, expect } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { emptyController } from '../src/sim/state';
import { addNativeCollision } from '../src/game/nativeCollision';
const available = existsSync('assets/courses/pss.json');
describe.skipIf(!available)('native course collision', () => {
  for (const id of ['bob', 'castle_grounds', 'castle_inside', 'wf', 'pss']) {
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
  it('resolves both front door warp nodes to walkable ground in both directions', () => {
    for (const [from, to] of [['castle_grounds', 'castle_inside'], ['castle_inside', 'castle_grounds']]) {
      const source = JSON.parse(readFileSync(`assets/courses/${from}.json`, 'utf8'));
      const dest = JSON.parse(readFileSync(`assets/courses/${to}.json`, 'utf8'));
      const world = new CollisionWorld(); addNativeCollision(world, dest); world.finalize();
      expect(source.doors).toHaveLength(2);
      for (const door of source.doors) {
        expect(door.destination).toBe(to);
        const arrival = dest.doors.find((d: {node: string}) => d.node === door.destNode);
        const yaw = arrival.yaw * Math.PI / 180;
        const side = Math.sign(Math.cos(yaw)) * (to === 'castle_inside' ? -1 : 1);
        const [x, y, z] = arrival.pos;
        const hit = world.findFloor(x, y + 100, z + 300 * side * Math.cos(yaw));
        expect(hit.surface).not.toBeNull();
        expect(hit.height).toBeCloseTo(y, -1);
      }
    }
  });
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
