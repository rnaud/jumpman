// Golden fixtures for the stationary/automatic/submerged/drowning action ports:
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.stationary.gen.test.ts`, then `cargo test` in native/.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import * as S from '../src/sim/state';
import { STATIONARY_ACTIONS } from '../src/sim/actions/stationary';
import { AUTOMATIC_ACTIONS } from '../src/sim/actions/automatic';
import { SUBMERGED_ACTIONS } from '../src/sim/actions/drowning';
import { submergedStep } from '../src/sim/actions/submerged';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

const FLAGS = [S.INPUT_NONZERO_ANALOG, S.INPUT_A_PRESSED, S.INPUT_OFF_FLOOR, S.INPUT_ABOVE_SLIDE, S.INPUT_A_DOWN,
  S.INPUT_B_PRESSED, S.INPUT_Z_DOWN, S.INPUT_Z_PRESSED];

type Kind = 'stationary' | 'automatic' | 'submerged' | 'drowning';
const scenarios: { name: string; kind: Kind; action: number; arg: number; frames: number; prob: number; yOff?: number; ceil?: boolean }[] = [];
const stationary = [
  S.ACT_IDLE, S.ACT_LAVA_BOOST_LAND, S.ACT_CROUCHING, S.ACT_START_CROUCHING, S.ACT_STOP_CROUCHING, S.ACT_START_CRAWLING,
  S.ACT_STOP_CRAWLING, S.ACT_BRAKING_STOP, S.ACT_BUTT_SLIDE_STOP, S.ACT_SLIDE_KICK_SLIDE_STOP, S.ACT_JUMP_LAND_STOP,
  S.ACT_DOUBLE_JUMP_LAND_STOP, S.ACT_FREEFALL_LAND_STOP, S.ACT_SIDE_FLIP_LAND_STOP, S.ACT_TRIPLE_JUMP_LAND_STOP,
  S.ACT_BACKFLIP_LAND_STOP, S.ACT_LONG_JUMP_LAND_STOP, S.ACT_GROUND_POUND_LAND, S.ACT_HOLD_BUTT_SLIDE_STOP, S.ACT_HOLD_IDLE,
  S.ACT_HOLD_JUMP_LAND_STOP, S.ACT_HOLD_FREEFALL_LAND_STOP, S.ACT_AIR_THROW_LAND, S.ACT_START_SLEEPING, S.ACT_SLEEPING,
  S.ACT_WAKING_UP,
];
for (const a of stationary) for (const prob of [0, 0.03, 0.15]) {
  scenarios.push({ name: `st_${a.toString(16)}_${prob}`, kind: 'stationary', action: a, arg: 0, frames: prob === 0 ? 700 : 80, prob });
}
for (const a of [S.ACT_LEDGE_GRAB, S.ACT_LEDGE_CLIMB_SLOW_1, S.ACT_LEDGE_CLIMB_FAST, S.ACT_START_HANGING, S.ACT_HANGING, S.ACT_HANG_MOVING])
  for (const prob of [0, 0.05, 0.2]) for (const ceil of [false, true]) {
    scenarios.push({ name: `au_${a.toString(16)}_${prob}_${ceil}`, kind: 'automatic', action: a, arg: 0, frames: 90, prob, ceil });
  }
for (const [a, arg] of [[S.ACT_WATER_IDLE, 0], [S.ACT_WATER_ACTION_END, 0], [S.ACT_BREASTSTROKE, 0], [S.ACT_BREASTSTROKE, 1],
  [S.ACT_FLUTTER_KICK, 0], [S.ACT_SWIMMING_END, 0], [S.ACT_WATER_PLUNGE, 0]])
  for (const prob of [0.05, 0.3]) for (const yOff of [60, 400, 900]) {
    scenarios.push({ name: `sw_${a.toString(16)}_${arg}_${prob}_${yOff}`, kind: 'submerged', action: a, arg, frames: 150, prob, yOff });
  }
for (const a of [S.ACT_DROWNING, S.ACT_WATER_DEATH]) scenarios.push({ name: `dr_${a.toString(16)}`, kind: 'drowning', action: a, arg: 0, frames: 200, prob: 0 });

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust fixtures: stationary', () => {
  it('writes action goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
    const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
    const result: Record<string, unknown> = {};
    scenarios.forEach((sc, si) => {
      const r = rng(1000 + si);
      const m = new MarioRuntime(world, { pos: data.spawn.pos, faceYaw: data.spawn.faceYaw });
      if (sc.kind === 'submerged') { m.pos[1] = m.floorHeight + (sc.yOff ?? 0); }
      m.setAction(sc.action, sc.arg);
      let ceilSlack = 0;
      if (sc.ceil) { ceilSlack = Math.floor(r() * 4); m.ceil = { type: 5 } as never; m.ceilHeight = m.pos[1] + 160 + ceilSlack; }
      else if (sc.kind === 'automatic') m.ceilHeight = m.floorHeight + 200;
      const table = sc.kind === 'stationary' ? STATIONARY_ACTIONS : sc.kind === 'automatic' ? AUTOMATIC_ACTIONS : SUBMERGED_ACTIONS;
      const script: number[][] = []; const frames: number[][] = [];
      for (let f = 0; f < sc.frames; f++) {
        if (sc.kind !== 'submerged' && !table[m.action]) break;
        let input = 0;
        for (const fl of FLAGS) if (r() < sc.prob) input |= fl;
        const intendedYaw = Math.floor(r() * 65536) - 32768;
        const stickX = Math.round((r() - 0.5) * 128), stickY = Math.round((r() - 0.5) * 128);
        const animFrame = (f * 7) % 60;
        script.push([input, intendedYaw, stickX, stickY, animFrame]);
        m.input = input; m.intendedYaw = intendedYaw; m.stickY = stickY;
        m.controller = { stickX, stickY, stickMag: 0, buttonDown: 0, buttonPressed: 0 } as never;
        m.animFrame = animFrame;
        if (sc.kind === 'submerged') submergedStep(m, m.floorHeight + 700);
        else table[m.action](m);
        frames.push([...m.pos, ...m.vel, ...m.faceAngle, m.action, m.actionState, m.actionTimer, m.actionArg, m.forwardVel,
          m.floorHeight, m.animLength, m.animFrame, m.swimStrength, m.swimYawVel, m.respawns]);
      }
      result[sc.name] = { kind: sc.kind, action: sc.action, arg: sc.arg, ceil: !!sc.ceil, ceilSlack, yOff: sc.yOff ?? 0, script, frames };
    });
    writeFileSync(`${out}/actions_stationary.json`, JSON.stringify(result));
  });
});
