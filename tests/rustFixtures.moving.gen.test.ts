// Generates moving-action goldens: `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.moving.gen.test.ts`
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { A_BUTTON, B_BUTTON, Z_TRIG, ACT_WALKING, ACT_CROUCH_SLIDE, ACT_BUTT_SLIDE, ACT_DIVE_SLIDE } from '../src/sim/state';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust moving fixtures', () => {
  it('writes moving-action scripted-run goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    // script entry: [stickX, stickY, stickMag, buttonDown, buttonPressed]
    const mk = (fn: (f: number, r: () => number) => [number, number, number, number]) => {
      const r = rng(4242);
      return Array.from({ length: 240 }, (_, f) => {
        const [sx, sy, down, pressed] = fn(f, r);
        return [sx, sy, Math.min(64, Math.hypot(sx, sy)), down, pressed];
      });
    };
    const scripts: Record<string, { action: number; script: number[][] }> = {
      // walk, reverse the stick (turning around), release (braking/decelerating)
      turn: { action: ACT_WALKING, script: mk(f => f < 50 ? [0, 64, 0, 0] : f < 90 ? [0, -64, 0, 0] : [0, 0, 0, 0]) },
      // walk then Z: crouch slide, then B kick / A jump while sliding
      crouch: { action: ACT_WALKING, script: mk(f => f < 40 ? [0, 64, 0, 0] : f === 40 ? [0, 64, Z_TRIG, Z_TRIG] : f < 70 ? [0, 0, Z_TRIG, 0] : [0, 0, 0, 0]) },
      // walk then B: dive / punch from the ground
      punch: { action: ACT_WALKING, script: mk(f => f < 40 ? [0, 64, 0, 0] : f === 40 ? [0, 64, B_BUTTON, B_BUTTON] : f === 80 ? [0, 0, B_BUTTON, B_BUTTON] : [0, 0, 0, 0]) },
      // walk with a randomly wandering stick
      wander: { action: ACT_WALKING, script: mk((f, r) => f < 30 ? [0, 64, 0, 0] : [Math.round((r() - 0.5) * 128), Math.round((r() - 0.5) * 128), 0, 0]) },
      // sliding actions entered directly
      crouch_slide: { action: ACT_CROUCH_SLIDE, script: mk(() => [0, 0, 0, 0]) },
      butt_slide: { action: ACT_BUTT_SLIDE, script: mk(() => [0, 0, 0, 0]) },
      dive_slide: { action: ACT_DIVE_SLIDE, script: mk(f => f < 5 ? [0, 64, 0, 0] : [0, 0, 0, 0]) },
      jump_press: { action: ACT_WALKING, script: mk(f => f < 40 ? [0, 64, 0, 0] : f === 40 ? [0, 64, A_BUTTON, A_BUTTON] : [0, 64, 0, 0]) },
    };
    const fixtures: Record<string, unknown> = {};
    for (const [name, { action, script }] of Object.entries(scripts)) {
      const spawn = { action, pos: data.spawn.pos, faceYaw: data.spawn.faceYaw };
      const m = new MarioRuntime(world, spawn);
      const frames: number[][] = [];
      for (const [stickX, stickY, stickMag, buttonDown, buttonPressed] of script) {
        m.step({ stickX, stickY, stickMag, buttonDown, buttonPressed }, 0);
        frames.push([...m.pos, ...m.vel, ...m.faceAngle, m.action, m.forwardVel, m.health]);
      }
      fixtures[name] = { spawn, script, frames };
    }
    writeFileSync(`${out}/mario_moving_bob.json`, JSON.stringify(fixtures));
  });
});
