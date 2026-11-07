// Generates airborne-action goldens: `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.airborne.gen.test.ts`
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { A_BUTTON, B_BUTTON, Z_TRIG, ACT_GROUP_AIRBORNE, ACT_JUMP, ACT_TRIPLE_JUMP, ACT_BACKFLIP, ACT_SIDE_FLIP,
  ACT_LONG_JUMP, ACT_DIVE, ACT_GROUND_POUND, ACT_SLIDE_KICK, ACT_STEEP_JUMP, ACT_FREEFALL, ACT_DOUBLE_JUMP,
  ACT_JUMP_KICK, ACT_SOFT_BONK, ACT_BACKWARD_AIR_KB, ACT_FORWARD_AIR_KB, ACT_LAVA_BOOST, ACT_BUTT_SLIDE_AIR } from '../src/sim/state';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust airborne fixtures', () => {
  it('writes airborne goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    const actions = [ACT_JUMP, ACT_DOUBLE_JUMP, ACT_TRIPLE_JUMP, ACT_BACKFLIP, ACT_SIDE_FLIP, ACT_LONG_JUMP, ACT_DIVE, ACT_GROUND_POUND,
      ACT_SLIDE_KICK, ACT_STEEP_JUMP, ACT_FREEFALL, ACT_JUMP_KICK, ACT_SOFT_BONK, ACT_BACKWARD_AIR_KB, ACT_FORWARD_AIR_KB, ACT_LAVA_BOOST, ACT_BUTT_SLIDE_AIR];
    const cases: unknown[] = [];
    let seed = 1;
    for (const action of actions) for (const fv of [0, 30, 55]) for (const y of [1500, 400]) {
      const r = rng(seed++);
      const script: number[][] = [];
      for (let f = 0; f < 80; f++) {
        let sx = Math.round((r() - 0.5) * 128), sy = Math.round((r() - 0.5) * 128), down = 0, pressed = 0;
        if (f < 20) { sx = 0; sy = 64; }
        if (r() < 0.1) { pressed |= A_BUTTON; down |= A_BUTTON; }
        if (f > 0 && f < 15) down |= A_BUTTON;
        if (r() < 0.06) { pressed |= B_BUTTON; down |= B_BUTTON; }
        if (r() < 0.04) { pressed |= Z_TRIG; down |= Z_TRIG; }
        script.push([sx, sy, Math.min(64, Math.hypot(sx, sy)), down, pressed]);
      }
      const spawn = { action, pos: [data.spawn.pos[0], y, data.spawn.pos[2]] as [number, number, number], faceYaw: data.spawn.faceYaw };
      const m = new MarioRuntime(world, spawn);
      m.forwardVel = fv;
      const frames: number[][] = [];
      for (const [stickX, stickY, stickMag, buttonDown, buttonPressed] of script) {
        m.step({ stickX, stickY, stickMag, buttonDown, buttonPressed }, 0);
        if ((m.action & 0x1c0) !== ACT_GROUP_AIRBORNE) break;
        frames.push([...m.pos, ...m.vel, ...m.faceAngle, m.action, m.forwardVel, m.health]);
      }
      cases.push({ spawn, fv, script, frames });
    }
    writeFileSync(`${out}/airborne_bob.json`, JSON.stringify(cases));
  });
});
