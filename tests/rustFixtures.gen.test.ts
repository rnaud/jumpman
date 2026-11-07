// Generates golden fixtures for the Rust port: `GEN_RUST_FIXTURES=1 pnpm vitest run tests/rustFixtures.gen.test.ts`
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { atan2s, coss, sins, toS16 } from '../src/sim/math';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { A_BUTTON, B_BUTTON, ACT_FREEFALL } from '../src/sim/state';

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
  it('writes MarioRuntime scripted-run goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    const r = rng(777);
    // Deterministic 300-frame script: [stickX, stickY, stickMag, buttonDown, buttonPressed]
    const script: number[][] = [];
    for (let f = 0; f < 300; f++) {
      let sx = 0, sy = 0, down = 0, pressed = 0;
      if (f >= 60 && f < 140) sy = 64;
      else if (f >= 140 && f < 200) { sx = 64; sy = 32; }
      else if (f >= 200) { sx = Math.round((r() - 0.5) * 128); sy = Math.round((r() - 0.5) * 128); }
      if (f === 100 || f === 150 || f === 230) { pressed |= A_BUTTON; }
      if ((f >= 100 && f < 112) || (f >= 150 && f < 160) || (f >= 230 && f < 245)) down |= A_BUTTON;
      if (f === 180 || f === 260) { pressed |= B_BUTTON; down |= B_BUTTON; }
      const mag = Math.min(64, Math.hypot(sx, sy));
      script.push([sx, sy, mag, down, pressed]);
    }
    const run = (spawn: { action?: number; pos: [number, number, number]; faceYaw: number }) => {
      const m = new MarioRuntime(world, spawn);
      const frames: number[][] = [];
      for (const [stickX, stickY, stickMag, buttonDown, buttonPressed] of script) {
        m.step({ stickX, stickY, stickMag, buttonDown, buttonPressed }, 0);
        frames.push([...m.pos, ...m.vel, ...m.faceAngle, m.action, m.forwardVel, m.health]);
      }
      return { spawn, script, frames };
    };
    const fixtures = {
      walk: run({ pos: data.spawn.pos, faceYaw: data.spawn.faceYaw }),
      freefall: run({ action: ACT_FREEFALL, pos: [data.spawn.pos[0], 1500, data.spawn.pos[2]], faceYaw: data.spawn.faceYaw }),
    };
    writeFileSync(`${out}/mario_run_bob.json`, JSON.stringify(fixtures));
  });
});
