// Long-run differential fixtures for the Rust sim: the full MarioRuntime + ObjectManager (course objects
// loaded the way sm64World.ts / courseObjects.ts do) driven for 3000 frames per course by a seeded
// pseudo-random controller. Per frame we store a bit-exact hash of Mario's state and of every object.
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.longrun.gen.test.ts`
// Compared by native/src/game/longrun_tests.rs. LONGRUN_DETAIL=<course> LONGRUN_OUT=<file> dumps full state.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { describe, it } from 'vitest';
import { CollisionWorld } from '../src/sim/collision';
import { addNativeCollision } from '../src/game/nativeCollision';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { ObjectManager } from '../src/sim/objectManager';
import { degToAngle, atan2s, toS16 } from '../src/sim/math';
import { A_BUTTON, B_BUTTON, Z_TRIG, ACT_FLYING, ACT_FREEFALL } from '../src/sim/state';

export const FRAMES = 3000;
export const COURSES: [string, number][] = [
  ['bob', 1], ['wf', 1], ['ccm', 1], ['bbh', 1], ['jrb', 1], ['castle_grounds', 1], ['castle_inside', 1],
  ['lll', 1], ['ssl', 1], ['hmc', 1], ['bitdw', 1], ['pss', 1], ['bob', 2], ['ttm', 1], ['sl', 1], ['wdw', 1],
  ['ccm', 5], ['ccm', 3], ['ddd', 1], ['thi', 1], ['ttc', 1], ['rr', 1], ['bitfs', 1], ['bits', 1], ['bowser_1', 1],
  ['bowser_2', 1], ['bowser_3', 1], ['sa', 1], ['totwc', 1], ['wf', 2], ['jrb', 2], ['hmc', 3], ['ssl', 2], ['lll', 2],
];

const hashBuf = new DataView(new ArrayBuffer(8));
class Hasher {
  h = 0x811c9dc5;
  word(w: number) { this.h = Math.imul(this.h ^ (w >>> 0), 0x01000193) >>> 0; }
  num(v: number) {
    if (typeof v !== 'number') throw new Error('hash field is not a number: ' + String(v));
    if (v !== v) { this.word(0x7ff80000); this.word(0); return; }
    hashBuf.setFloat64(0, v, true);
    this.word(hashBuf.getUint32(0, true)); this.word(hashBuf.getUint32(4, true));
  }
  str(s: string) { for (let i = 0; i < s.length; i++) this.word(s.charCodeAt(i)); this.word(0xff); }
}

function marioFields(m: any, mgr: ObjectManager): number[] {
  const idx = (o: any) => (o ? mgr.objects.indexOf(o) : -1);
  return [
    ...m.pos, ...m.vel, ...m.faceAngle, m.forwardVel, m.action, m.prevAction, m.actionState, m.actionTimer, m.actionArg,
    m.health, m.hurtCounter, m.healCounter, m.slideVelX, m.slideVelZ, m.slideYaw, m.peakHeight, m.squishTimer, m.floorHeight,
    m.ceilHeight, m.intendedYaw, m.intendedMag, m.flags, m.input, m.invincTimer, m.wingCapTimer, m.metalCapTimer,
    m.vanishCapTimer, idx(m.heldObj), idx(m.usedObj), idx(m.interactObj), m.polePos, m.poleYawVelocity, m.twirlYaw,
    m.quicksandDepth, m.doubleJumpTimer, m.wallKickTimer, m.angleVel[0], m.angleVel[1], m.angleVel[2], m.lives, m.gravityScale,
    m.shellRoll, m.grabTimer, idx(m.ridingShell), idx(m.grabbedBy), m.framesSinceA, m.framesSinceB, m.particleFlags,
  ];
}
export const MARIO_FIELD_COUNT = 50;

function objectFields(o: any): (number | string)[] {
  const vars = Object.entries(o.vars as Record<string, unknown>)
    .filter(([, v]) => typeof v === 'number' && Number.isFinite(v)).sort(([a], [b]) => (a < b ? -1 : 1));
  return [
    o.behavior, ...o.pos, ...o.vel, o.moveYaw, o.faceYaw, o.forwardVel, o.action, o.actionTimer, o.active ? 1 : 0,
    o.scale, o.hitbox.radius, o.hitbox.height, o.timer, o.heldState, o.grounded ? 1 : 0, o.hitWall ? 1 : 0, o.interactStatus,
    o.visible ? 1 : 0, o.graphYOffset, o.distanceToMario, ...vars.flatMap(([k, v]) => [k, v as number]),
  ];
}

function hashFields(list: (number | string)[], hasher = new Hasher()) {
  for (const v of list) typeof v === 'string' ? hasher.str(v) : hasher.num(v);
  return hasher;
}

/** Integer-only LCG so Rust reproduces the controller script exactly. */
function inputScript(seed: number, frames: number) {
  let s = seed >>> 0;
  const next = () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) >>> 16);
  const out: { x: number; y: number; mag: number; down: number; pressed: number; yaw: number }[] = [];
  let prevDown = 0;
  while (out.length < frames) {
    const len = 6 + (next() % 50);
    const still = next() % 5 === 0;
    const x = still ? 0 : (next() % 129) - 64, y = still ? 0 : (next() % 129) - 64;
    const mag = Math.min(64, Math.sqrt(x * x + y * y));
    let down = 0;
    if (next() % 100 < 45) down |= A_BUTTON;
    if (next() % 100 < 25) down |= B_BUTTON;
    if (next() % 100 < 15) down |= Z_TRIG;
    const yaw = (next() % 64) * 1024 - 32768;
    const pulse = 3 + (next() % 12);
    for (let i = 0; i < len && out.length < frames; i++) {
      // Tap pattern: A/B toggle every `pulse` frames so presses (edges) occur, Z stays held.
      let d = down;
      if (((i / pulse) | 0) % 2 === 1) d &= Z_TRIG;
      out.push({ x, y, mag, down: d, pressed: d & ~prevDown, yaw });
      prevDown = d;
    }
  }
  return out;
}

export function loadCourse(id: string, act: number) {
  const data = JSON.parse(readFileSync(`assets/courses/${id}.json`, 'utf8'));
  if (id === 'bob') data.objects.push({ model: 'levels/bob/bob_geo_000458.glb', pos: [-2303, 717, 1024], yaw: 45 });
  for (const warp of data.warps ?? []) data.objects.push({ ...warp, model: '', yaw: 0, behavior: 'warp' });
  const activeObjects = data.objects.filter((o: any) => !o.acts || o.acts.length === 0 || o.acts.includes(act));
  if (data.snowmanStar && act !== data.snowmanStar.act) {
    const body = data.objects.find((o: any) => o.behavior === 'snowmanBody');
    if (body) activeObjects.push({ ...body, param: -1, pos: [-4230, -1344, 1813] });
  }
  const collision = new CollisionWorld(undefined, data.horizontalBoundary);
  collision.slideTerrain = !!data.slideTerrain;
  collision.snowTerrain = data.snowTerrain ?? false;
  collision.waterBoxes = (data.waterBoxes ?? []).map((b: number[]) => [...b]);
  addNativeCollision(collision, data);
  // Static object collision (sm64World.ts prepareArea). Moving platforms, door colliders and the
  // ship parts are app-level and not part of this sim differential.
  const extras: Record<string, string> = {
    'levels/bob/bob_geo_000458.glb': 'seesaw_platform/collision.inc.c',
    'levels/wf/wf_geo_0008A8.glb': 'areas/1/10/collision.inc.c',
    'levels/wf/wf_geo_0008E8.glb': 'areas/1/11/collision.inc.c',
  };
  const platformKinds = ['checkerboard_lift', 'ropeway_lift', 'tower_solid', 'tower_slide', 'tower_elevator', 'seesaw_platform',
    'rotating_platform', 'rotating_wooden_platform', 'sliding_platform', 'small_bomp', 'large_bomp', 'mesh_elevator', 'merry_go_round',
    'moving_bookshelf', 'tilting_trap_platform'];
  for (const object of activeObjects) {
    if (object.behavior) continue;
    if (object.groundedProp) {
      const [x, y, z] = object.pos;
      const hit = collision.findFloor(x, y + 150, z);
      if (hit.surface) object.pos[1] = hit.height;
    }
    const key = object.collision ?? extras[object.model];
    const extra = data.extraCollision?.[key];
    const kind = (key ?? '').split('/')[0];
    if (object.dungeonRole || (object.submarine && extra) || !extra || platformKinds.includes(kind)) continue;
    addNativeCollision(collision, extra, object.pos, object.yaw);
  }
  collision.finalize?.();
  let spawn = JSON.parse(JSON.stringify(data.spawn));
  // Total Wing Cap is entered through its flying warp (prepareArea: default entry WARP_NODE_0A).
  if (id === 'totwc') spawn = { pos: data.warpSpawns.WARP_NODE_0A.pos, faceYaw: data.warpSpawns.WARP_NODE_0A.faceYaw, action: ACT_FLYING };
  if (!spawn.action) {
    const [sx, sy, sz] = spawn.pos;
    const hit = collision.findFloor(sx, sy + 200, sz);
    if (hit.surface) spawn.pos[1] = hit.height;
  }
  const specs = [
    ...activeObjects.filter((o: any) => o.behavior),
    ...(data.mountainBallSpawners ?? []).filter((s: any) => s.acts.includes(act)).map((s: any) => ({
      pos: s.pos, model: '', yaw: 0, behavior: 'mountainBallSpawner',
      ballRoute: { waypoints: data.mountainBallPaths![s.path], period: s.period, maxDistance: s.path === 0 ? 7000 : 6000 },
    })),
    ...(data.waterBombSpawners ?? []).map((spec: any) => ({ ...spec, model: '', yaw: 0, behavior: 'waterBombSpawner' })),
  ].map((o: any) => ({
    behavior: o.behavior, ballRoute: o.ballRoute, pos: o.pos, yaw: degToAngle(o.yaw), param: o.param ?? 0, model: o.model,
    warpTo: o.warpTo, warpArea: o.warpArea, warpEntry: o.warpEntry,
  }));
  const deathHeight = Math.min(...data.vertices.map((v: number[]) => v[1])) - 500;
  return { data, collision, spawn, specs, deathHeight };
}

function runCourse(id: string, act: number, index: number, detail: boolean) {
  const { data, collision, spawn, specs, deathHeight } = loadCourse(id, act);
  let rs = (0x1234abcd + index * 7919) >>> 0;
  Math.random = () => ((rs = (Math.imul(rs, 1664525) + 1013904223) >>> 0) / 4294967296);
  const mario = new MarioRuntime(collision, spawn, deathHeight);
  const mgr = new ObjectManager(collision, specs, data.penguinRace, data.snowmanPath);
  const script = inputScript(0x9e3779b9 + index * 104729, FRAMES);
  const frames: number[] = [];
  const actions = new Set<number>();
  const dump: unknown[] = [];
  for (let f = 0; f < FRAMES; f++) {
    const c = script[f];
    mgr.step(mario);
    const e1 = mgr.events.length;
    mgr.drainEvents();
    mario.step({ stickX: c.x, stickY: c.y, stickMag: c.mag, buttonDown: c.down, buttonPressed: c.pressed }, c.yaw);
    mgr.afterMarioStep(mario);
    const e2 = mgr.events.length;
    mgr.drainEvents();
    const mf = marioFields(mario, mgr);
    const mh = hashFields([...mf, e1, e2]);
    const oh = new Hasher();
    oh.word(mgr.objects.length);
    for (const o of mgr.objects) hashFields(objectFields(o), oh);
    frames.push(mh.h, oh.h);
    actions.add(mario.action);
    if (detail) dump.push({ mario: mf, e1, e2, objects: mgr.objects.map(objectFields) });
  }
  return {
    id, act, spawn, deathHeight, frames, actions: actions.size, objectCount: specs.length,
    kinds: specs.map((s: any) => [s.behavior, ...s.pos, s.yaw, s.param]), dump,
  };
}


// ---------------------------------------------------------------------------------------------
// Focus fuzz: Mario is dropped next to each object in turn (unique kinds first), walks at it and then
// button-mashes for FOCUS_WINDOW frames. This exercises the interaction paths (poles, grabbing and
// throwing, chain chomp, penguins, snowman, whomps, ...) that a random walk from the spawn rarely reaches.
export const FOCUS_WINDOW = 120;
export const FOCUS_COURSES: [string, number, boolean?][] = [
  ['bob', 1], ['ccm', 1], ['ccm', 3], ['ccm', 5], ['wf', 1], ['ssl', 1], ['bbh', 1], ['hmc', 1], ['jrb', 1], ['lll', 1],
  ['castle_grounds', 1], ['bitdw', 1], ['sl', 1], ['wdw', 1], ['ttm', 1], ['thi', 1], ['bowser_1', 1], ['ddd', 1],
  // Snowman's Lost His Head already collected: the finished snowman (hidden body + head on it) path.
  ['ccm', 1, true],
];

function focusOrder(specs: any[]): number[] {
  const seen = new Set<string>(), first: number[] = [], rest: number[] = [];
  specs.forEach((sp, i) => {
    if (sp.behavior === 'warp') return;
    if (seen.has(sp.behavior)) rest.push(i); else { seen.add(sp.behavior); first.push(i); }
  });
  return [...first, ...rest];
}

function runFocus(id: string, act: number, index: number, snowmanDone = false) {
  const { data, collision, spawn, specs, deathHeight } = loadCourse(id, act);
  let rs = (0x77aa55cc + index * 6151) >>> 0;
  Math.random = () => ((rs = (Math.imul(rs, 1664525) + 1013904223) >>> 0) / 4294967296);
  const mario = new MarioRuntime(collision, spawn, deathHeight);
  const mgr = new ObjectManager(collision, specs, data.penguinRace, data.snowmanPath);
  if (snowmanDone) {
    // CourseObjects.setSnowmanCompleted(true)
    const body = mgr.findByBehavior('snowmanBody'), head = mgr.findByBehavior('snowmanHead');
    if (head && body?.param === -1) {
      body.visible = true; head.pos = [...head.home]; head.vars.placed = 1; head.pos[1] = -994; head.setAction(1);
    }
  }
  const order = focusOrder(specs);
  let s = (0xabc12345 + index * 7717) >>> 0;
  const next = () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) >>> 16);
  const base = atan2s(-64, 0);
  const frames: number[] = [];
  const actions = new Set<number>();
  let prevDown = 0;
  let seg: { x: number; y: number; mag: number; down: number; pulse: number; yaw: number; left: number; i: number } | undefined;
  for (let f = 0; f < FRAMES; f++) {
    const k = f % FOCUS_WINDOW;
    if (k === 0 && order.length) {
      const t = mgr.objects[order[(f / FOCUS_WINDOW) % order.length]];
      const dx = ((next() % 9) - 4) * 70, dz = ((next() % 9) - 4) * 70;
      mario.pos = [t.pos[0] + dx, t.pos[1] + 100, t.pos[2] + dz];
      mario.vel = [0, 0, 0];
      mario.forwardVel = 0;
      mario.faceAngle[1] = atan2s(t.pos[2] - mario.pos[2], t.pos[0] - mario.pos[0]);
      mario.setAction(ACT_FREEFALL, 0);
      seg = undefined;
    }
    let c;
    if (k < 45 && order.length) {
      const t = mgr.objects[order[Math.floor(f / FOCUS_WINDOW) % order.length]];
      const target = atan2s(t.pos[2] - mario.pos[2], t.pos[0] - mario.pos[0]);
      // Walk at it; tap A early so poles and ledges get grabbed from the air too.
      const d = k % 20 === 5 || k % 20 === 6 ? A_BUTTON : 0;
      c = { x: 0, y: 64, mag: 64, down: d, pressed: d & ~prevDown, yaw: toS16(target - base) };
    } else {
      if (!seg || seg.left === 0) {
        const len = 6 + (next() % 30), still = next() % 6 === 0;
        const x = still ? 0 : (next() % 129) - 64, y = still ? 0 : (next() % 129) - 64;
        let down = 0;
        if (next() % 100 < 45) down |= A_BUTTON;
        if (next() % 100 < 40) down |= B_BUTTON;
        if (next() % 100 < 15) down |= Z_TRIG;
        seg = { x, y, mag: Math.min(64, Math.sqrt(x * x + y * y)), down, pulse: 3 + (next() % 10), yaw: (next() % 64) * 1024 - 32768, left: len, i: 0 };
      }
      let d = seg.down;
      if (((seg.i / seg.pulse) | 0) % 2 === 1) d &= Z_TRIG;
      // A deliberate B tap every 14 frames: picks things up, then throws what is held.
      if (k % 14 === 0) d |= B_BUTTON;
      seg.i++; seg.left--;
      c = { x: seg.x, y: seg.y, mag: seg.mag, down: d, pressed: d & ~prevDown, yaw: seg.yaw };
    }
    prevDown = c.down;
    mgr.step(mario);
    const e1 = mgr.events.length;
    mgr.drainEvents();
    mario.step({ stickX: c.x, stickY: c.y, stickMag: c.mag, buttonDown: c.down, buttonPressed: c.pressed }, c.yaw);
    mgr.afterMarioStep(mario);
    const e2 = mgr.events.length;
    mgr.drainEvents();
    const mh = hashFields([...marioFields(mario, mgr), e1, e2]);
    const oh = new Hasher();
    oh.word(mgr.objects.length);
    for (const o of mgr.objects) hashFields(objectFields(o), oh);
    frames.push(mh.h, oh.h);
    actions.add(mario.action);
  }
  return { id, act, frames, actions: [...actions] };
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust long-run fixtures', () => {
  it('writes long-run goldens', () => {
    const realRandom = Math.random;
    try {
      const only = process.env.LONGRUN_DETAIL;
      const out: unknown[] = [];
      COURSES.forEach(([id, act], i) => {
        const r = runCourse(id, act, i, only === `${id}.${act}`);
        console.log(id, act, 'objects', r.objectCount, 'distinct actions', r.actions);
        if (only === `${id}.${act}`) writeFileSync(process.env.LONGRUN_OUT!, JSON.stringify(r.dump));
        out.push({ ...r, dump: undefined });
      });
      mkdirSync('native/sim/tests/fixtures', { recursive: true });
      const focus = FOCUS_COURSES.map(([id, act, done], i) => {
        const r = { ...runFocus(id, act, i, !!done), snowmanDone: !!done };
        console.log('focus', id, act, 'distinct actions', r.actions.length);
        return r;
      });
      writeFileSync('native/sim/tests/fixtures/focus.json', JSON.stringify({ frames: FRAMES, window: FOCUS_WINDOW, courses: focus }));
      writeFileSync('native/sim/tests/fixtures/longrun.json', JSON.stringify({ frames: FRAMES, marioFields: MARIO_FIELD_COUNT, courses: out }));
    } finally { Math.random = realRandom; }
  }, 600000);
});
