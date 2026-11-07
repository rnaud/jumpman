// Golden fixtures for the Bob-omb/boss/box behavior ports:
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.bobs.gen.test.ts`, then `cargo test -p jumpman-sim`.
import { mkdirSync, writeFileSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ObjectManager } from '../src/sim/objectManager';
import { LootCoin } from '../src/sim/lootCoin';
import { GrandStar, SpawnedStar } from '../src/sim/spawnedStar';
import type { ObjectSpec } from '../src/sim/object';
import { ACT_GROUND_POUND_LAND, ACT_READING_NPC_DIALOG, ACT_IDLE } from '../src/sim/state';

function rng(seed: number) { let s = seed >>> 0; return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296); }

type V3 = [number, number, number];
/** [x, y, z, yaw, action|null, actionState|null] */
type MarioFrame = [number, number, number, number, number | null, number | null];
interface Poke { frame: number; obj: number; field: string; value: number }
interface Scenario {
  name: string; seed: number; specs: ObjectSpec[]; frames: number; mario: (f: number) => MarioFrame; pokes?: Poke[];
}

function makeWorld(): CollisionWorld {
  const w = new CollisionWorld();
  const R = 4000;
  w.addTriangle([-R, 0, -R], [-R, 0, R], [R, 0, R]);
  w.addTriangle([-R, 0, -R], [R, 0, R], [R, 0, -R]);
  // A wall across z = 1200 facing -z, and one across x = -1500 facing +x.
  w.addQuad([-2000, 0, 1200], [2000, 0, 1200], [2000, 800, 1200], [-2000, 800, 1200]);
  w.addQuad([-1500, 0, -2000], [-1500, 0, 2000], [-1500, 800, 2000], [-1500, 800, -2000]);
  w.finalize();
  return w;
}

const spec = (behavior: any, pos: V3, yaw = 0, extra: Partial<ObjectSpec> = {}): ObjectSpec => ({ behavior, pos, yaw, ...extra });
const still = (x: number, y: number, z: number, yaw = 0): ((f: number) => MarioFrame) => () => [x, y, z, yaw, null, null];

const scenarios: Scenario[] = [
  {
    name: 'bobomb_boxes', seed: 11, frames: 330,
    specs: [
      spec('bobomb', [900, 0, 0]),
      spec('bobomb', [-700, 0, 300], 0x4000, { param: 1 }),
      spec('bobomb', [0, 0, -900]),
      spec('breakableBox', [-300, 0, -600], 0, { param: 1 }),
      spec('breakableBoxSmall', [300, 0, 600]),
      spec('breakableBoxSmall', [600, 0, -600]),
    ],
    pokes: [
      { frame: 30, obj: 3, field: 'interactStatus', value: 1 << 14 },
      { frame: 20, obj: 4, field: 'heldState', value: 2 },
      { frame: 60, obj: 1, field: 'interactStatus', value: 1 << 14 },
      { frame: 90, obj: 5, field: 'heldState', value: 3 },
      { frame: 100, obj: 2, field: 'heldState', value: 1 },
    ],
    mario: f => [Math.sin(f / 40) * 200, 0, Math.cos(f / 50) * 150, 0x2000, null, null],
  },
  {
    name: 'king_bobomb', seed: 5, frames: 700,
    specs: [spec('kingBobomb', [0, 0, 0], 0x4000), spec('bobomb', [800, 0, 800])],
    pokes: [
      { frame: 10, obj: 0, field: 'vars.dialogDone', value: 1 },
      { frame: 200, obj: 0, field: 'heldState', value: 2 },
      { frame: 330, obj: 0, field: 'heldState', value: 3 },
      { frame: 460, obj: 0, field: 'heldState', value: 2 },
      { frame: 520, obj: 0, field: 'vars.dialogDone', value: 1 },
      { frame: 650, obj: 0, field: 'vars.dialogDone', value: 1 },
    ],
    mario: f => [f < 5 ? 700 : 400 + Math.sin(f / 30) * 100, 0, f < 5 ? 0 : 100 * Math.cos(f / 25), 0, null, null],
  },
  {
    name: 'chain_chomp', seed: 3, frames: 700,
    specs: [spec('chainChomp', [0, 0, 0]), spec('chainChompGate', [3288, 0, -1770]), spec('bobomb', [-500, 0, -500])],
    pokes: [{ frame: 160, obj: 0, field: 'vars.release', value: 1 }],
    mario: f => f < 40 ? [1500, 0, 0, 0, null, null]
      : f < 130 ? [300 + 400 * Math.sin(f / 15), 0, 400 * Math.cos(f / 15), 0x1000, null, null]
      : f < 300 ? [400, 0, 500, 0x1000, null, f >= 190 ? 8 : null]
      : [400, 0, 500, 0x1000, null, null],
  },
  {
    name: 'water_cannon', seed: 21, frames: 500,
    specs: [
      spec('waterBombCannon', [0, 0, 0], 0x2000),
      spec('waterBombCannon', [1500, 0, 1500], 0, { param: 1 }),
      spec('waterBombSpawner', [-1000, 0, -1000], 0, { param: 2 }),
    ],
    mario: f => f < 200 ? [900, 0, 300, 0x2000, null, null] : [-900 + (f - 200) * 0.3, 0, -1000, 0x4000, null, null],
  },
  {
    name: 'bowser', seed: 8, frames: 500,
    specs: [spec('bowser', [0, 0, 0], 0, { param: 3 }), spec('bowserMine', [500, 0, 0]), spec('bowserMine', [-900, 0, 200]), spec('bowser', [2500, 0, 2500], 0, { param: 1 })],
    pokes: [
      { frame: 90, obj: 0, field: 'vars.spin', value: 3000 },
      { frame: 90, obj: 0, field: 'heldState', value: 2 },
      { frame: 250, obj: 3, field: 'heldState', value: 2 },
    ],
    mario: f => [1800 * Math.cos(f / 60), 0, 1800 * Math.sin(f / 60), 0x5000, null, null],
  },
  {
    name: 'bob_objects', seed: 31, frames: 600,
    specs: [
      spec('purpleSwitch', [200, 0, -300]),
      spec('openableGrill', [-800, 0, -800], 0x4000),
      spec('pitBowlingBall', [1000, 0, 1000], 0x2000),
      spec('mountainBallSpawner', [-300, 800, 300], 0, { ballRoute: { waypoints: [[-300, 800, 300], [300, 500, 500], [1000, 100, 1200], [1500, 0, 1800]], period: 8, maxDistance: 9000 } }),
    ],
    mario: f => f < 50 ? [-1000, 0, -600, 0, null, null] : f < 400 ? [200, 0, -300, 0, null, null] : [-300, 0, 0, 0, null, null],
  },
  {
    name: 'boxes_chests', seed: 2, frames: 500,
    specs: [
      spec('starBox', [0, 0, 600]),
      spec('wingBox', [600, 0, 600]),
      spec('chest', [-600, 0, 0], 0, { param: 1 }),
      spec('chest', [-600, 0, 400], 0, { param: 2 }),
      spec('chest', [-600, 0, -400], 0, { param: 1 }),
      spec('movingYellowCoin', [100, 100, 100]),
    ],
    pokes: [
      { frame: 5, obj: 0, field: 'interactStatus', value: 1 << 14 },
      { frame: 10, obj: 1, field: 'vars.unlocked', value: 1 },
      { frame: 40, obj: 1, field: 'interactStatus', value: 1 << 14 },
    ],
    mario: f => f < 60 ? [0, 0, 0, 0, null, null]
      : f < 70 ? [100, 104, 0, 0, null, null]
      : f < 120 ? [-600, 0, -300, 0x8000, null, null]
      : f < 130 ? [-600, 0, 100, 0x8000, null, null]
      : f < 150 ? [-600, 0, 400 + 100, 0x8000, null, null]
      : f < 200 ? [600, 0, 700, 0, null, null]
      : [-600, 0, -300, 0x8000, null, null],
  },
];

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust bob fixtures', () => {
  it('writes behavior goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const result: any = {};
    for (const sc of scenarios) {
      Math.random = rng(sc.seed);
      const world = makeWorld();
      const mario = new MarioRuntime(world, { pos: [0, 0, 0], faceYaw: 0 });
      const mgr = new ObjectManager(world, sc.specs);
      const frames: any[] = [];
      const surfaces: number[] = [];
      for (let f = 0; f < sc.frames; f++) {
        const [x, y, z, yaw, action, actionState] = sc.mario(f);
        mario.pos[0] = x; mario.pos[1] = y; mario.pos[2] = z; mario.faceAngle[1] = yaw; mario.floorHeight = 0;
        if (action !== null) mario.action = action;
        if (actionState !== null) mario.actionState = actionState;
        for (const p of sc.pokes ?? []) {
          if (p.frame !== f) continue;
          const o: any = mgr.objects[p.obj];
          if (p.field.startsWith('vars.')) o.vars[p.field.slice(5)] = p.value; else o[p.field] = p.value;
        }
        mgr.step(mario);
        const num = (n: number) => (Number.isFinite(n) ? n : null);
        frames.push({
          m: [mario.health, mario.action, ...mario.pos, mario.actionState].map(num),
          o: mgr.objects.map(o => [...o.pos, ...o.vel, o.forwardVel, o.moveYaw, o.faceYaw, o.action, o.actionState, o.actionTimer,
            o.timer, o.active ? 1 : 0, o.visible ? 1 : 0, o.heldState, o.scale, o.graphYOffset, o.grounded ? 1 : 0, o.interactStatus,
            o.hitbox.interactType, o.hitbox.radius, o.hitbox.height, o.coinDropped ? 1 : 0].map(num)),
          v: mgr.objects.map(o => Object.fromEntries(Object.entries(o.vars).filter(([, v]) => typeof v === 'number').map(([k, v]) => [k, num(v as number)]))),
          e: mgr.events.map(e => { const a: any = e; return [e.type, a.pos ? a.pos.map(num) : null, String(a.phase ?? a.cue ?? a.id ?? a.number ?? a.act ?? a.value ?? '')]; }),
        });
        surfaces.push(world.surfaces.length);
      }
      const kinds = new Set(frames.flatMap(fr => fr.e.map((e: any) => e[0])));
      console.log(sc.name, 'objects', mgr.objects.length, 'events', [...kinds].join(','), 'size', JSON.stringify(frames).length);
      result[sc.name] = {
        specs: sc.specs, pokes: sc.pokes ?? [], seed: sc.seed,
        mario: Array.from({ length: sc.frames }, (_, f) => sc.mario(f)), frames, surfaces,
      };
    }

    // Pure classes: LootCoin and the spawned/grand stars.
    Math.random = rng(77);
    const world = makeWorld();
    const coins = [] as any[];
    for (let c = 0; c < 4; c++) {
      const coin = new LootCoin();
      const pos: V3 = [100 * c, 50, -200 * c];
      const trace: number[][] = [];
      for (let i = 0; i < 460; i++) {
        const alive = coin.step(pos, world);
        trace.push([...pos, alive ? 1 : 0, coin.tangible ? 1 : 0, coin.visible ? 1 : 0]);
        if (!alive) break;
      }
      coins.push({ start: [100 * c, 50, -200 * c], trace });
    }
    const stars: any = { spawned: [], grand: [] };
    for (const mario of [undefined, [600, 100, -400] as V3]) {
      const s = new SpawnedStar([100, 300, 100], mario);
      const trace: number[][] = [];
      for (let i = 0; i < 120; i++) { s.step(); trace.push([...s.pos, s.yaw, s.tangible ? 1 : 0, s.returning ? 1 : 0, s.bouncing ? 1 : 0]); }
      stars.spawned.push({ mario: mario ?? null, trace });
    }
    const g = new GrandStar([800, 200, -500]);
    const gt: number[][] = [];
    for (let i = 0; i < 200; i++) { g.step(); gt.push([...g.pos, g.tangible ? 1 : 0]); }
    stars.grand.push({ trace: gt });
    result.loot = { coins, seed: 77 };
    result.stars = stars;
    writeFileSync(`${out}/bob_behaviors.json`, JSON.stringify(result));
  });
});
