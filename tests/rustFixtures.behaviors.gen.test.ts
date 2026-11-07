// Generates golden fixtures for the Rust port of the world-actor behaviors (koopa, penguins, snowman,
// whomp, boo, bully, pokey, ...): `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.behaviors.gen.test.ts`
// Compared by native/sim/tests/behaviors_golden.rs.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ObjectManager } from '../src/sim/objectManager';
import type { ObjectSpec, PenguinRace } from '../src/sim/object';
import { ACT_GROUND_POUND_LAND, ACT_IDLE } from '../src/sim/state';
import type { Vec3 } from '../src/sim/math';

type Set = [number, number, string, number];
interface Scenario {
  world: 'bob' | 'flat';
  specs: ObjectSpec[];
  race?: PenguinRace;
  snowmanPath?: Vec3[];
  frames: number;
  mario: Vec3;
  /** Pin Mario to the script position every frame (scenario A). */
  follow: boolean;
  sets: Set[];
}

function flatWorld(): CollisionWorld {
  const w = new CollisionWorld();
  w.addTriangle([-7000, 0, -7000], [-7000, 0, 7000], [7000, 0, 7000]);
  w.addTriangle([-7000, 0, -7000], [7000, 0, 7000], [7000, 0, -7000]);
  w.finalize();
  return w;
}

function bobWorld(): CollisionWorld {
  const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
  const world = new CollisionWorld(); addNativeCollision(world, data); world.finalize();
  return world;
}

const STATE_VARS = (o: any) => Object.entries(o.vars as Record<string, unknown>)
  .filter(([, v]) => typeof v === 'number' && Number.isFinite(v)).sort(([a], [b]) => (a < b ? -1 : 1));

function snapshot(mgr: ObjectManager, mario: MarioRuntime) {
  return {
    mario: [...mario.pos, mario.action, mario.health],
    events: mgr.events.length,
    objects: mgr.objects.map(o => [
      o.behavior, ...o.pos, ...o.vel, o.moveYaw, o.faceYaw, o.forwardVel, o.action, o.actionTimer, o.active ? 1 : 0,
      o.scale, o.hitbox.radius, o.hitbox.height, o.timer, o.heldState, o.grounded ? 1 : 0, o.hitWall ? 1 : 0, o.interactStatus,
      o.visible ? 1 : 0, o.graphYOffset, o.distanceToMario, STATE_VARS(o),
    ]),
  };
}

function run(sc: Scenario) {
  const world = sc.world === 'bob' ? bobWorld() : flatWorld();
  const mario = new MarioRuntime(world, { pos: [...sc.mario] as Vec3, faceYaw: 0 });
  const mgr = new ObjectManager(world, sc.specs, sc.race, sc.snowmanPath);
  mario.pos = [...sc.mario] as Vec3;
  const marioScript: number[][] = [];
  const frames: ReturnType<typeof snapshot>[] = [];
  for (let f = 0; f < sc.frames; f++) {
    let attack = false;
    if (sc.follow) {
      const k = Math.floor(f / 20) % sc.specs.length;
      const t = mgr.objects[k];
      mario.pos = [t.pos[0] + 120, t.pos[1], t.pos[2]];
      attack = f % 20 >= 8 && f % 20 < 12;
    }
    mario.action = attack ? ACT_GROUND_POUND_LAND : ACT_IDLE;
    mario.health = 0x880;
    for (const [sf, idx, key, value] of sc.sets) if (sf === f) mgr.objects[idx].vars[key] = value;
    marioScript.push([...mario.pos, mario.action]);
    mgr.step(mario);
    frames.push(snapshot(mgr, mario));
  }
  return { ...sc, marioScript, snapshots: frames };
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust behavior fixtures', () => {
  it('writes behavior goldens', () => {
    const out = 'native/sim/tests/fixtures'; mkdirSync(out, { recursive: true });
    const realRandom = Math.random;
    Math.random = () => 0.37;
    try {
      const data = JSON.parse(readFileSync('assets/courses/bob.json', 'utf8'));
      const bob = bobWorld();
      const kinds: [ObjectSpec['behavior'], number?][] = [
        ['koopa'], ['koopa'], ['boo'], ['bigBoo'], ['bully'], ['bigBully'], ['pokey'], ['mrBlizzard'], ['klepto'], ['whomp'],
        ['whompKing'], ['penguin'], ['babyPenguin', 3], ['tuxiesMother', 3], ['bobombBuddy'], ['oneUp'], ['flame'], ['poisonGas'], ['tree'],
        ['koopaShell'], ['babyPenguin', 4],
      ];
      const specs: ObjectSpec[] = kinds.map(([behavior, param], i) => {
        const a = (i / kinds.length) * Math.PI * 2;
        const x = data.spawn.pos[0] + Math.sin(a) * 700, z = data.spawn.pos[2] + Math.cos(a) * 700;
        const hit = bob.findFloor(x, data.spawn.pos[1] + 500, z);
        return { behavior, pos: [x, hit.surface ? hit.height : data.spawn.pos[1], z] as Vec3, yaw: i * 3000, param };
      });
      const scenarios: Record<string, Scenario> = {
        bob_actors: { world: 'bob', specs, frames: kinds.length * 20, mario: data.spawn.pos, follow: true, sets: [] },
        quick_race: {
          world: 'flat', frames: 400, mario: [0, 0, 260], follow: false,
          specs: [
            { behavior: 'koopaTheQuick', pos: [0, 0, 0], yaw: 0 },
            { behavior: 'koopaFlag', pos: [200, 0, 300], yaw: 0 },
          ],
          sets: [[3, 0, 'answer', 1]],
        },
        penguin_race: {
          world: 'flat', frames: 400, mario: [0, 0, 50], follow: false,
          specs: [{ behavior: 'racingPenguin', pos: [0, 0, 0], yaw: 0 }],
          race: {
            path: Array.from({ length: 50 }, (_, i) => [i, 150 * i, 0, 0] as [number, number, number, number]),
            finish: [7000, 0, 0], shortcut: [0, 0, 5000],
          },
          sets: [[3, 0, 'answer', 1]],
        },
        snowman: {
          world: 'flat', frames: 300, mario: [0, 0, 1900], follow: false,
          specs: [
            { behavior: 'snowmanBody', pos: [0, 0, 2000], yaw: 0, param: 0 },
            { behavior: 'snowmanHead', pos: [600, 0, 2000], yaw: 0x4000 },
          ],
          snowmanPath: [[0, 0, 2000], [0, 0, 1000], [-500, 0, 0], [-1000, 0, -1500], [-1000, 0, -3000]],
          sets: [],
        },
        koopa_kick: {
          world: 'flat', frames: 300, mario: [0, 0, 400], follow: false,
          specs: [{ behavior: 'koopa', pos: [0, 0, 0], yaw: 0 }, { behavior: 'koopa', pos: [900, 0, 0], yaw: 0x4000 }],
          sets: [[10, 0, 'hit', 2], [60, 0, 'hit', 3], [120, 0, 'hit', 4], [20, 1, 'hit', 3], [90, 1, 'hit', 5]],
        },
      };
      const fixtures: Record<string, unknown> = {};
      for (const [name, sc] of Object.entries(scenarios)) fixtures[name] = run(sc);
      writeFileSync(`${out}/behaviors.json`, JSON.stringify(fixtures));
    } finally {
      Math.random = realRandom;
    }
  });
  it('writes Math.sin/cos goldens', () => {
    let s = 99;
    const r = () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) / 4294967296);
    const xs: number[] = [];
    for (let i = 0; i < 4000; i++) xs.push((r() - 0.5) * 14);
    for (let i = 0; i < 2000; i++) xs.push((r() - 0.5) * 2e5);
    for (let i = 0; i < 2000; i++) xs.push((Math.floor(r() * 131072) - 65536) * Math.PI * 2 / 65536);
    for (let i = 0; i < 1000; i++) xs.push(i * 0.012 + 0.012);
    writeFileSync('native/sim/tests/fixtures/js_math.json', JSON.stringify({ xs, sin: xs.map(Math.sin), cos: xs.map(Math.cos) }));
  });
});
