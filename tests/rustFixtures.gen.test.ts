// Generates golden fixtures for the Rust port: `GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { atan2s, coss, sins, toS16 } from '../src/sim/math';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust fixtures', () => {
  it('writes math + collision goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const r = rng(1234);
    const angles = Array.from({ length: 500 }, () => Math.floor(r() * 200000) - 100000);
    const pairs = Array.from({ length: 500 }, () => [Math.round((r() - 0.5) * 4000), Math.round((r() - 0.5) * 4000)]);
    writeFileSync(`${out}/math.json`, JSON.stringify({
      angles, sins: angles.map(sins), coss: angles.map(coss), s16: angles.map(toS16),
      pairs, atan: pairs.map(([z, x]) => atan2s(z, x)),
    }));
    for (const id of ['bob', 'castle_inside']) {
      const data = JSON.parse(readFileSync(`assets/courses/${id}.json`, 'utf8'));
      const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
      const xs = data.vertices.map((v: number[]) => v[0]), ys = data.vertices.map((v: number[]) => v[1]), zs = data.vertices.map((v: number[]) => v[2]);
      const lo = [Math.min(...xs), Math.min(...ys), Math.min(...zs)], hi = [Math.max(...xs), Math.max(...ys), Math.max(...zs)];
      const index = new Map(world.surfaces.map((s, i) => [s, i]));
      const q = Array.from({ length: 3000 }, () => [0, 1, 2].map(i => lo[i] + r() * (hi[i] - lo[i]) + (i === 1 ? 300 : 0)));
      writeFileSync(`${out}/collision_${id}.json`, JSON.stringify({
        queries: q,
        floors: q.map(([x, y, z]) => { const h = world.findFloor(x, y, z); return [h.height, h.surface ? index.get(h.surface) : -1]; }),
        ceils: q.map(([x, y, z]) => { const h = world.findCeil(x, y, z); return [h.height, h.surface ? index.get(h.surface) : -1]; }),
        walls: q.map(([x, y, z]) => { const p: [number, number, number] = [x, y - 250, z]; const n = world.f32FindWallCollision(p, 60, 50); return [n, ...p]; }),
      }));
    }
  });
});
