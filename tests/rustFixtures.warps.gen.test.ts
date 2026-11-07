// Warp differential generator for the Rust port: area-changing instant warps (ddd, wdw, castle_inside), the warp
// resolution of objects / doors / paintings / death / fall / floor warps (what Game.ts asks loadCourse, changeLinkedArea
// or the act selector for, and on which tick), and the arrival state (spawn + 60 frames) for every (course, entry,
// push, act, area) a warp can load. Writes native/sim/tests/fixtures/warps.json.
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.warps.gen.test.ts`; compared by native/src/progress/warps_tests.rs.
import { readFileSync, writeFileSync, mkdirSync, readdirSync } from 'node:fs';
import { describe, it, vi } from 'vitest';
import * as THREE from 'three';

vi.mock('../src/game/sm64Assets', async () => {
  const T = await import('three');
  const stub = () => {
    const group = new T.Group();
    group.add(new T.Group());
    return group;
  };
  return {
    sm64AssetPaths: [],
    SM64_MODELS: {
      mario: 'actors/mario/mario_geo.glb', battlefield: 'levels/bob/bob_geo_000488.glb',
      redCoin: 'actors/coin/red_coin_no_shadow_geo.glb', coin: 'actors/coin/yellow_coin_no_shadow_geo.glb',
      transparentStar: 'actors/transparent_star/transparent_star_geo.glb', star: 'actors/star/star_geo.glb',
      tree: 'actors/tree/bubbly_tree_geo.glb', goomba: 'actors/goomba/goomba_geo.glb',
    },
    sm64AssetUrl: (path: string) => path,
    sourceUnitScale: () => 100,
    loadSm64Asset: async () => ({ scene: new T.Group(), animations: [...Array.from({ length: 24 }, (_, i) => new T.AnimationClip('stub' + i, 1, [])), new T.AnimationClip('Open', 1, [])], asset: { extras: {} } }),
    instantiateSm64: stub,
    Sm64Billboards: class { register() {} update() {} get size() { return 0; } },
  };
});
vi.mock('../src/game/actorShadows', () => ({
  ActorShadows: class { register() {} remove() {} update() {} dispose() {} },
}));
vi.mock('../src/game/castlePaintings', () => ({
  buildCastlePaintings: async () => ({ showRoom() {}, reset() {}, dispose() {}, step() {} }),
}));

import { NpcDialogCamera } from '../src/game/NpcDialogCamera';
import { kingDialogPages } from '../src/game/kingDialog';
import { CoinEffects } from '../src/game/coinEffects';
import { KoopaRace } from '../src/sim/behaviors/koopaTheQuick';
import { prepareCourse } from '../src/game/sm64World';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { SaveFile } from '../src/game/save';
import { crossesGoomba, touchesGoombaHurtbox, type GoombaEnemy } from '../src/game/goomba';
import { readableSign, SignReading } from '../src/game/signpost';
import { CannonOperator } from '../src/game/cannonOperator';
import { paintingDestination } from '../src/game/painting';
import { swordReach } from '../src/sim/linkController';
import { marioHitbox, marioOverlapsObject } from '../src/sim/object';
import { atan2s, sins, coss, toS16, type Vec3 } from '../src/sim/math';
import type { Surface } from '../src/sim/collision';
import {
  A_BUTTON, B_BUTTON, Z_TRIG, ACT_FREEFALL, ACT_TELEPORT_FADE_OUT, ACT_TELEPORT_FADE_IN, ACT_IDLE, ACT_CROUCHING, ACT_FLAG_INTANGIBLE, ACT_FLAG_INVULNERABLE, ACT_FLAG_AIR, ACT_FLAG_ATTACKING,
  ACT_FLAG_IDLE, ACT_WALKING, ACT_GROUND_POUND, ACT_GROUND_POUND_LAND, ACT_READING_NPC_DIALOG, ACT_IN_CANNON,
  MARIO_PUNCHING, MARIO_KICKING, MARIO_TRIPPING, MARIO_UNKNOWN_08, type ControllerSnapshot,
} from '../src/sim/state';


import { COURSES, SCORE_COURSES, courseSeries } from '../src/game/courses';
import { prepareCourse } from '../src/game/sm64World';
import { MarioRuntime } from '../src/sim/MarioRuntime';
import { paintingDestination } from '../src/game/painting';
import type { Surface } from '../src/sim/collision';
import { A_BUTTON, ACT_FREEFALL, ACT_FLAG_INTANGIBLE, type ControllerSnapshot } from '../src/sim/state';

// ---- environment stubs -------------------------------------------------------------------------
function installStubs() {
  (THREE.TextureLoader.prototype as any).load = function () { return new THREE.Texture(); };
  const el = () => ({ className: '', setAttribute() {}, append() {}, remove() {}, style: {} as any, hidden: true, textContent: '' });
  (globalThis as any).document = { createElement: el, body: { append() {} } };
  (globalThis as any).fetch = async (url: string) => {
    const path = String(url).split('?')[0].replace(/^\/@fs/, '');
    const file = path.includes('assets/courses/') ? 'assets/courses/' + path.split('assets/courses/')[1] : path;
    return { ok: true, status: 200, json: async () => JSON.parse(readFileSync(file, 'utf8')) };
  };
}

// ---- hashing ------------------------------------------------------------------------------------
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
type Field = number | string;
function hashFields(list: Field[], hasher = new Hasher()) {
  for (const v of list) typeof v === 'string' ? hasher.str(v) : hasher.num(v);
  return hasher;
}

function marioFields(m: any, objects: any[]): number[] {
  const idx = (o: any) => (o ? objects.indexOf(o) : -1);
  return [
    ...m.pos, ...m.vel, ...m.faceAngle, m.forwardVel, m.action, m.prevAction, m.actionState, m.actionTimer, m.actionArg,
    m.health, m.hurtCounter, m.healCounter, m.slideVelX, m.slideVelZ, m.slideYaw, m.peakHeight, m.squishTimer, m.floorHeight,
    m.ceilHeight, m.intendedYaw, m.intendedMag, m.flags, m.input, m.invincTimer, m.wingCapTimer, m.metalCapTimer,
    m.vanishCapTimer, idx(m.heldObj), idx(m.usedObj), idx(m.interactObj), m.polePos, m.poleYawVelocity, m.twirlYaw,
    m.quicksandDepth, m.doubleJumpTimer, m.wallKickTimer, m.angleVel[0], m.angleVel[1], m.angleVel[2], m.lives, m.gravityScale,
    m.shellRoll, m.grabTimer, idx(m.ridingShell), idx(m.grabbedBy), m.framesSinceA, m.framesSinceB, m.particleFlags,
    m.cannon ? 1 : 0, m.cannon ? m.cannon.home[0] : 0, m.cannon ? m.cannon.cooldown : 0,
    m.floor ? m.floor.type : -1, m.floor ? (m.floor.room ?? -1) : -2,
  ];
}

function objectFields(o: any): Field[] {
  const vars = Object.entries(o.vars as Record<string, unknown>)
    .filter(([, v]) => typeof v === 'number' && Number.isFinite(v)).sort(([a], [b]) => (a < b ? -1 : 1));
  return [
    o.behavior, ...o.pos, ...o.vel, o.moveYaw, o.faceYaw, o.forwardVel, o.action, o.actionTimer, o.active ? 1 : 0,
    o.scale, o.hitbox.radius, o.hitbox.height, o.timer, o.heldState, o.grounded ? 1 : 0, o.hitWall ? 1 : 0, o.interactStatus,
    o.visible ? 1 : 0, o.graphYOffset, o.distanceToMario, ...vars.flatMap(([k, v]) => [k, v as number]),
  ];
}

const surfaceCache = new WeakMap<Surface, number>();
function surfaceHash(s: Surface, ownerIdx: number): number {
  const h = new Hasher();
  for (const v of [s.v1, s.v2, s.v3]) for (const c of v) h.num(c);
  h.num(s.type); h.num(s.force); h.num(s.flags); h.num(s.room ?? -1); h.num(ownerIdx);
  return h.h;
}
/** Order-independent hash of every surface in the world (plus the surface count). */
function worldHash(world: any, objects: any[]): number {
  let sum = 0;
  for (const s of world.surfaces as Surface[]) {
    let h = surfaceCache.get(s);
    const owner = s.object ? objects.indexOf(s.object) : -1;
    if (h === undefined || owner !== -1) {
      h = surfaceHash(s, owner);
      if (owner === -1) surfaceCache.set(s, h);
    }
    sum = (sum + h) >>> 0;
  }
  const out = new Hasher();
  out.word(world.surfaces.length); out.word(sum);
  return out.h;
}



// ================================================================================================
// Scenarios.
// ================================================================================================
const COURSE_DIR = 'assets/courses/';
const FRAMES = 60;

function integerLcg(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) >>> 16);
}

async function setup(id: string, act: number, area: number, index: number, entry?: string | null, push = false) {
  let rs = (0x2badbeef + index * 7919) >>> 0;
  const realRandom = Math.random;
  Math.random = () => (new Error().stack!.includes('generateUUID') ? realRandom() : ((rs = (Math.imul(rs, 1664525) + 1013904223) >>> 0) / 4294967296));
  const factory = await prepareCourse(id as any, entry ?? null, push, act, area);
  const world = factory(new THREE.Scene());
  const mario = new MarioRuntime(world.collision, world.spawn, world.deathHeight!);
  return { world, mario, restore: () => { Math.random = realRandom; } };
}

function c(x: number, y: number, down: number, prev: number): ControllerSnapshot {
  return { stickX: x, stickY: y, stickMag: Math.min(64, Math.sqrt(x * x + y * y)), buttonDown: down, buttonPressed: down & ~prev };
}

function allObjects(objects: any[]): number {
  const all = new Hasher();
  all.word(objects.length);
  for (const o of objects) hashFields(objectFields(o), all);
  return all.h;
}

/** One frame of the shared script: object step, Mario step, after-step (the part of Game.stepWorld every course runs). */
function stepFrame(world: any, mario: any, controller: ControllerSnapshot) {
  if (mario.invincTimer > 0) mario.invincTimer--;
  const events = world.objects?.step(mario, undefined, false) ?? [];
  mario.step(controller, 0);
  world.objects?.afterMarioStep(mario);
  return events as any[];
}

function frameHashes(world: any, mario: any): number[] {
  const objects = (world.objects?.manager.objects ?? []) as any[];
  return [hashFields(marioFields(mario, objects)).h, allObjects(objects), worldHash(world.collision, objects)];
}

// ---- A. arrivals: every (course, entry, push, act, area) a warp can load --------------------------------------
interface Tuple { id: string; entry: string | null; push: boolean; act: number; area: number }
const tupleKey = (t: Tuple) => `${t.id}|${t.entry ?? '-'}|${t.push ? 1 : 0}|${t.act}|${t.area}`;

async function arrivalScenario(t: Tuple, index: number) {
  const { world, mario, restore } = await setup(t.id, t.act, t.area, index, t.entry, t.push);
  try {
    const frames: number[] = [];
    const next = integerLcg(500 + index);
    let prevDown = 0;
    for (let f = 0; f < FRAMES; f++) {
      let down = 0, sx = 0, sy = 0;
      if (f > 20) {
        const mag = next() % 3 === 0 ? 0 : 64;
        sx = mag ? (next() % 129) - 64 : 0; sy = mag ? (next() % 129) - 64 : 0;
        if (next() % 7 === 0) down |= A_BUTTON;
      }
      const controller = c(sx, sy, down, prevDown);
      prevDown = down;
      stepFrame(world, mario, controller);
      frames.push(...frameHashes(world, mario));
    }
    return { name: 'arrival ' + tupleKey(t), tuple: t, frames, groups: 3, stats: { pos: mario.pos.map(Math.round), action: mario.action } };
  } finally { restore(); }
}

// ---- B. trigger resolution (what Game.ts asks loadCourse / changeLinkedArea / the act selector for) -------------
type Resolution = string;
const load = (course: string, entry: string | null | undefined, push: boolean, act: number, area: number): Resolution =>
  `load:${course}:${entry ?? '-'}:${push ? 1 : 0}:${act}:${area}`;
const select = (course: string, entry: string | null | undefined): Resolution => `select:${course}:${entry ?? '-'}`;

/** `enterCourseWarp` (painting / warp object) */
function enterCourse(dest: string, entry?: string): Resolution {
  return SCORE_COURSES.has(dest as any) ? select(dest, entry) : load(dest, entry, false, 1, 1);
}

/** `case 'warp'` of processObjectEvents */
function objectWarp(courseId: string, linkedArea: number | undefined, event: { destination: string; entry?: string; area?: number }): Resolution {
  if (event.destination === courseId && event.area && event.area === linkedArea) return `swap:${event.area}:${event.entry ?? '-'}:0`;
  if (!Object.hasOwn(COURSES, event.destination)) return 'none';
  return enterCourse(event.destination, event.entry);
}

/** The `tick === 36` branch of the door cutscene. */
function doorWarp(courseId: string, linkedArea: number | undefined, door: any, push: boolean): Resolution {
  if (door.destination === courseId && door.destArea === linkedArea) return `swap:${door.destArea}:${door.destNode}:${push ? 1 : 0}`;
  return load(door.destination, door.destNode, push, 1, door.destArea);
}

async function staticResolutions(files: string[], tuples: Map<string, Tuple>) {
  const out: { name: string; value: Resolution }[] = [];
  const add = (t: Tuple) => { tuples.set(tupleKey(t), t); };
  const parseLoad = (r: Resolution) => {
    const [, id, entry, push, act, area] = r.split(':');
    if (r.startsWith('load:')) add({ id, entry: entry === '-' ? null : entry, push: push === '1', act: Number(act), area: Number(area) });
  };
  for (const file of files) {
    const id = file.replace(/(\.\d)?\.json$/, '');
    const area = Number(file.match(/\.(\d)\.json$/)?.[1] ?? 1);
    if (id === 'playground' || id === 'mushroom2d' || !Object.hasOwn(COURSES, id) || courseSeries(id) === 'Zelda') continue;
    const data = JSON.parse(readFileSync(COURSE_DIR + file, 'utf8'));
    // the TS world tells which areas are linked (ddd / ccm)
    const { world, restore } = await setup(id, 1, area, 0);
    restore();
    const linked: number | undefined = world.linkedArea?.world.area;
    const warps = [...(data.warps ?? []), ...(data.objects ?? []).filter((o: any) => o.warpTo)];
    warps.forEach((w: any, i: number) => {
      const r = objectWarp(id, linked, { destination: w.warpTo, entry: w.warpEntry, ...(w.warpArea ? { area: w.warpArea } : {}) });
      out.push({ name: `warp ${file} #${i}`, value: r }); parseLoad(r);
    });
    (data.doors ?? []).forEach((d: any, i: number) => {
      for (const push of [false, true]) {
        const r = doorWarp(id, linked, d, push);
        out.push({ name: `door ${file} #${i} push${push ? 1 : 0}`, value: r }); parseLoad(r);
      }
    });
    // death / fall / floor warps as resolved by beginDeathWarp + finishDeathWarp / the jrb floor warp
    for (const fall of [false, true]) {
      const target = (fall && data.fallWarp) || data.deathWarp;
      if (!target) continue;
      const act = 3;
      const r = load(target.course, target.entry, false, target.course === id ? act : 1, target.area);
      out.push({ name: `${fall ? 'fall' : 'death'} ${file}`, value: r }); parseLoad(r);
    }
    if (data.floorWarp) {
      const r = load(id, data.floorWarp.entry, false, 2, data.floorWarp.area);
      out.push({ name: `floor ${file}`, value: r }); parseLoad(r);
    }
    if (id === 'totwc') { const r = load('castle_inside', 'WARP_NODE_20', false, 1, 1); out.push({ name: 'totwc fall', value: r }); parseLoad(r); }
  }
  return out;
}

// ---- C. painting warps on the real floors -------------------------------------------------------------------------
async function paintingScenarios(index: number) {
  const out: { name: string; value: Resolution }[] = [];
  for (const area of [1, 2, 3]) {
    const { world, mario, restore } = await setup('castle_inside', 1, area, index + area);
    try {
      const byType = new Map<number, Surface>();
      for (const s of world.collision.surfaces as Surface[]) if (s.type >= 0xd3 && s.type < 0xd3 + 0x2d && !byType.has(s.type)) byType.set(s.type, s);
      for (const [type, s] of [...byType].sort((a, b) => a[0] - b[0])) {
        mario.pos = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3, Math.max(s.v1[1], s.v2[1], s.v3[1]) + 80 * (type % 2), (s.v1[2] + s.v2[2] + s.v3[2]) / 3];
        mario.vel = [0, 0, 0]; mario.forwardVel = 0; mario.setAction(ACT_FREEFALL, 0);
        let found = 'none@never';
        for (let f = 0; f < 6; f++) {
          const events = stepFrame(world, mario, c(0, 0, 0, 0));
          if (process.env.WARP_DUMP === 'paint' && type === 0xff) console.log('PT', area, f, mario.pos.map(Math.round), mario.floor?.type, Math.round(mario.floorHeight), mario.action.toString(16));
          const dest = mario.action & ACT_FLAG_INTANGIBLE ? null : paintingDestination(mario.floor?.type ?? -1, mario.pos[1] - mario.floorHeight, world.paintings);
          if (dest) { found = enterCourse(dest, 'WARP_NODE_0A') + '@' + f; break; }
          const warp = events.find(e => e.type === 'warp');
          if (warp) { found = objectWarp('castle_inside', undefined, warp) + '@' + f; break; }
        }
        out.push({ name: `painting area${area} type${type.toString(16)} y${type % 2}`, value: found, index: index + area });
      }
    } finally { restore(); }
  }
  return out;
}

// ---- D. death, fall and floor warps (trigger tick, target, carried state) ----------------------------------------------
function carryOf(m: any) { return `${m.health},${m.hurtCounter},${m.healCounter}`; }

async function deathScenario(id: string, area: number, mode: 'die' | 'fall' | 'plane', lives: number, index: number) {
  const fall = mode !== 'die';
  const act = id === 'totwc' ? 1 : 2;
  const { world, mario, restore } = await setup(id, act, area, index);
  try {
    let req: { tick: number; fall: boolean } | undefined;
    let tick = 0;
    mario.lives = lives;
    mario.requestDeathWarp = f => {
      const target = (f && world.fallWarp) || world.deathWarp;
      if (!target) return false;
      req ??= { tick, fall: f };
      return true;
    };
    if (fall) {
      mario.pos[1] = mode === 'plane' ? -6000 : world.deathHeight! + 30; mario.vel = [0, -60, 0]; mario.setAction(ACT_FREEFALL, 0);
    } else mario.health = 0xff;
    let result = 'none';
    for (tick = 1; tick <= 400 && !req; tick++) stepFrame(world, mario, c(0, 0, 0, 0));
    const frames: number[] = [];
    if (req) {
      const target = (req.fall && world.fallWarp) || world.deathWarp!;
      const gameOver = mario.lives === 0 && !(req.fall && world.fallWarp);
      const keep = req.fall && target.course === id && world.fallWarp;
      result = gameOver ? 'gameover' : load(target.course, target.entry, false, target.course === id ? act : 1, target.area);
      // load request lands (duration + 1) ticks after the tick that asked for it
      result += `|ask@${req.tick}|fall${req.fall ? 1 : 0}|load@${req.tick + (req.fall ? 20 : 48) + 1}|carry${keep ? 1 : 0}:${carryOf(mario)}`;
    }
    return { name: `death ${id}.${area} ${mode} lives${lives}`, value: result };
  } finally { restore(); }
}

async function floorWarpScenario(kind: 'jrb' | 'totwc', index: number) {
  const { world, mario, restore } = await setup(kind, 1, 1, index);
  try {
    const frames: number[] = [];
    let timer = 0, value = 'none';
    const fw = world.floorWarp;
    mario.health = 0x500; mario.hurtCounter = 4; mario.healCounter = 8;
    if (kind === 'jrb') {
      // No floor of type 0x32 is a movement floor in the exported jrb collision (the pit falls out of the world and takes
      // the fall warp instead), so the warp floor is forced: every surface of the spawn's floor gets the type.
      const hit = world.collision.findFloor(mario.pos[0], mario.pos[1] + 100, mario.pos[2]);
      const old = hit.surface!;
      world.collision.removeSurfaces(new Set([old]));
      world.collision.addTriangle(old.v1, old.v2, old.v3, { type: 0x32 as any, room: old.room, force: old.force, flags: old.flags });
      mario.setAction(ACT_FREEFALL, 0);
    } else {
      mario.pos[1] = -6000;
      mario.vel = [0, 0, 0]; mario.forwardVel = 0; mario.setAction(ACT_FREEFALL, 0);
    }
    const next = integerLcg(8 + index);
    stepFrame(world, mario, c(0, 0, 0, 0)); // settle the floor
    for (let f = 1; f <= 400; f++) {
      if (kind === 'totwc' && (timer || mario.floor?.type === 0x0a && mario.pos[1] < mario.floorHeight + 2048)) {
        if (++timer === 20) { value = `load:castle_inside:WARP_NODE_20|tick${f}|carry:${carryOf(mario)}`; break; }
        frames.push(...frameHashes(world, mario));
        continue;
      }
      if (kind === 'jrb' && fw && (timer || mario.floor?.type === 0x32)) {
        if (++timer === 20) { value = `${load('jrb', fw.entry, false, 1, fw.area)}|tick${f}|carry:${carryOf(mario)}`; break; }
      }
      stepFrame(world, mario, c(next() % 2 ? 0 : 30, 0, 0, 0));
      if (process.env.WARP_DUMP && (f < 3 || f % 20 === 0)) console.log('FW', kind, f, mario.pos.map(Math.round), mario.floor?.type, mario.action.toString(16), timer);
      frames.push(...frameHashes(world, mario));
    }
    return { name: `floorwarp ${kind}`, value, frames };
  } finally { restore(); }
}

// ---- E. instant warps that change area (Game.stepWorld shift block + changeLinkedArea) ------------------------------------
async function instantAreaScenario(id: string, area: number, index: number) {
  let { world, mario, restore } = await setup(id, 1, area, index);
  try {
    const frames: number[] = [];
    const surfaceOf = (w: any) => {
      const warps = w.instantWarps ?? {};
      return (w.collision.surfaces as Surface[]).filter(s => s.type >= 0x1b && warps[s.type - 0x1b]);
    };
    let surfaces = surfaceOf(world);
    let shifts = 0, swaps = 0, hits = 0;
    const log: string[] = [];
    for (let f = 0; f < 360; f++) {
      const shift = world.instantWarps?.[(mario.floor?.type ?? 0) - 0x1b];
      if (shift) {
        if (shift.area !== world.area) {
          const linked = world.linkedArea;
          if (linked && linked.world.area === shift.area) {
            if (world.objects?.manager.penguinRace) world.objects.manager.resetRacer();
            world = linked.world;
            mario.changeArea(world.collision, world.spawn, world.deathHeight ?? -1600, shift.delta);
            surfaces = surfaceOf(world); swaps++;
            log.push(`${f}:${world.area}`);
          }
        } else {
          for (let axis = 0; axis < 3; axis++) mario.pos[axis] += shift.delta[axis];
          shifts++;
        }
      }
      if (f % 60 === 0 && surfaces.length) {
        const s = surfaces[(f / 60 * 3) % surfaces.length];
        mario.pos = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3, Math.max(s.v1[1], s.v2[1], s.v3[1]) + 60, (s.v1[2] + s.v2[2] + s.v3[2]) / 3];
        mario.vel = [0, 0, 0]; mario.forwardVel = 0; mario.setAction(ACT_FREEFALL, 0);
        hits++;
      }
      stepFrame(world, mario, c(0, 0, 0, 0));
      frames.push(...frameHashes(world, mario), world.area ?? 1, shifts);
    }
    return { name: `instant ${id}.${area}`, frames, groups: 5, stats: { shifts, swaps, hits, surfaces: surfaces.length, log } };
  } finally { restore(); }
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust warp fixtures', () => {
  it('writes warps goldens', async () => {
    installStubs();
    const files = readdirSync(COURSE_DIR).filter(f => f.endsWith('.json')).sort();
    const tuples = new Map<string, Tuple>();
    const resolutions = await staticResolutions(files, tuples);
    resolutions.push(...await paintingScenarios(100));
    let i = 200;
    for (const [id, area, fall, lives] of [['bob', 1, 'die', 3], ['bob', 1, 'die', 0], ['bob', 1, 'fall', 3], ['jrb', 1, 'fall', 3], ['jrb', 2, 'die', 3], ['ddd', 1, 'die', 3], ['ddd', 2, 'fall', 3],
      ['totwc', 1, 'fall', 3], ['totwc', 1, 'plane', 3], ['totwc', 1, 'die', 3], ['pss', 1, 'fall', 3], ['castle_grounds', 1, 'die', 3], ['castle_grounds', 1, 'fall', 3], ['bitdw', 1, 'die', 3], ['bits', 1, 'fall', 3]] as const) {
      const r = await deathScenario(id, area, fall, lives, i);
      resolutions.push({ name: r.name, value: r.value, index: i++ });
    }
    const scenarios: any[] = [];
    for (const k of ['jrb'] as const) {
      const r = await floorWarpScenario(k, i);
      resolutions.push({ name: r.name, value: r.value, index: i });
      scenarios.push({ name: r.name, frames: r.frames, groups: 3, index: i++ });
    }
    for (const [id, area] of [['ddd', 1], ['ddd', 2], ['wdw', 1], ['castle_inside', 2]] as const) scenarios.push({ ...await instantAreaScenario(id, area, i), index: i++ });
    const sorted = [...tuples.values()].sort((a, b) => (tupleKey(a) < tupleKey(b) ? -1 : 1));
    for (const [n, t] of sorted.entries()) scenarios.push({ ...await arrivalScenario(t, 1000 + n), index: 1000 + n });
    for (const r of resolutions) console.log(r.name, '=>', r.value);
    for (const s of scenarios) console.log(s.name, s.frames.length, JSON.stringify(s.stats ?? {}));
    mkdirSync('native/sim/tests/fixtures', { recursive: true });
    writeFileSync('native/sim/tests/fixtures/warps.json', JSON.stringify({ resolutions, scenarios }));
  }, 1800000);
});
