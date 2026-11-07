// App-layer differential fixtures for the Rust port: the REAL world factory (`prepareCourse`, with the glb
// loader stubbed out) builds moving platforms, door colliders, walking goombas, cannons, room doors,
// coin formations and the CourseObjects colliders; a copy of the relevant part of `Game.stepWorld` drives
// them together with MarioRuntime + ObjectManager for GAME_FRAMES frames. Mario is teleported next to one
// "feature" (platform, goomba, cannon, buddy, door, sign, painting floor, coin) every GAME_WINDOW frames,
// walks at it and then mashes buttons. Per frame we store bit-exact hashes of seven state groups.
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.game.gen.test.ts`
// Compared by native/src/game/game_tests.rs. GAME_DETAIL=<course>.<act> GAME_OUT=<file> dumps full state.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
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
    loadSm64Asset: async () => ({ scene: new T.Group(), animations: Array.from({ length: 24 }, (_, i) => new T.AnimationClip('stub' + i, 1, [])), asset: { extras: {} } }),
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
  A_BUTTON, B_BUTTON, Z_TRIG, ACT_FREEFALL, ACT_FLAG_INTANGIBLE, ACT_FLAG_INVULNERABLE, ACT_FLAG_AIR, ACT_FLAG_ATTACKING,
  ACT_FLAG_IDLE, ACT_WALKING, ACT_GROUND_POUND, ACT_GROUND_POUND_LAND, ACT_READING_NPC_DIALOG, ACT_IN_CANNON,
  MARIO_PUNCHING, MARIO_KICKING, MARIO_TRIPPING, MARIO_UNKNOWN_08, type ControllerSnapshot,
} from '../src/sim/state';

export const GAME_FRAMES = 2400;
export const GAME_WINDOW = 150;
/** [course, act, area, stars] */
export const GAME_COURSES: [string, number, number, number][] = [
  ['bob', 1, 1, 0], ['bob', 2, 1, 0], ['wf', 1, 1, 0], ['ccm', 1, 1, 0], ['ccm', 3, 1, 0], ['bbh', 1, 1, 0], ['jrb', 1, 1, 0],
  ['castle_inside', 1, 1, 12], ['castle_inside', 1, 2, 12], ['castle_inside', 1, 3, 40], ['castle_grounds', 1, 1, 0],
  ['ssl', 1, 1, 0], ['lll', 1, 1, 0], ['ttc', 1, 1, 0], ['bitdw', 1, 1, 0], ['bits', 1, 1, 0], ['bitfs', 1, 1, 0],
  ['ttm', 1, 1, 0], ['rr', 1, 1, 0], ['sl', 1, 1, 0], ['thi', 1, 1, 0], ['wdw', 1, 1, 0], ['hmc', 1, 1, 0],
  ['sa', 1, 1, 0], ['pss', 1, 1, 0], ['totwc', 1, 1, 0], ['ddd', 1, 1, 0], ['wf', 2, 1, 0],
  // Other acts and linked areas: act-specific objects/cannons, the Jolly Roger Bay ship, ddd's submarine, ccm's second area.
  ['jrb', 2, 1, 0], ['jrb', 1, 2, 0], ['ddd', 1, 2, 0], ['ccm', 1, 2, 0], ['bob', 3, 1, 0], ['wf', 3, 1, 0], ['ssl', 2, 1, 0], ['lll', 2, 1, 0],
  ['castle_inside', 1, 1, 0], ['castle_inside', 1, 1, 70],
];

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

// ---- state vectors ---------------------------------------------------------------------------------
function platformFields(p: any): number[] {
  const m = p.model;
  return [m.position.x, m.position.y, m.position.z, m.rotation.x, m.rotation.y, m.rotation.z, m.visible ? 1 : 0,
    p.waypoint, p.moveYaw, p.pitchVelocity, p.pitch, p.tick, p.phase, p.speed, p.checkerFacePitch];
}
function goombaFields(g: any): number[] {
  const m = g.model;
  return [m.position.x, m.position.y, m.position.z, m.rotation.y, m.visible ? 1 : 0, g.dead ? 1 : 0, g.vy, g.squashFrame,
    m.scale.x, m.scale.y, m.scale.z,
    g.yaw ?? 0, g.target ?? 0, g.speed ?? 0, g.relativeSpeed ?? 0, g.walkTimer ?? 0, g.airborne ? 1 : 0, g.awake ? 1 : 0];
}
function coinFields(c: any): number[] {
  const l = c.loot as any;
  return [c.position.x, c.position.y, c.position.z, c.collected ? 1 : 0, c.value ?? 1, c.group.visible ? 1 : 0, c.nativeHitbox ? 1 : 0,
    l ? 1 : 0, l ? (l.tangible ? 1 : 0) : 0, l ? (l.visible ? 1 : 0) : 0, l ? l.age : 0, l ? l.yaw : 0, l ? l.speed : 0,
    l ? l.verticalSpeed : 0, l ? l.bounce : 0, l ? (l.grounded ? 1 : 0) : 0];
}

interface Feature { kind: string; pos: Vec3; yOffset?: number; spread?: number; frontYaw?: number; frontDist?: number }

function integerLcg(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) >>> 16);
}

async function runGame(id: string, act: number, area: number, stars: number, index: number, detail: boolean) {
  const realRandom = Math.random;
  let rs = (0x51ed270b + index * 9973) >>> 0;
  // three.js draws UUIDs from Math.random for every Object3D/material/geometry it creates; those draws are
  // presentation-only and must not shift the sim's stream (which Rust reproduces), so they get the real one.
  Math.random = () => (new Error().stack!.includes('generateUUID') ? realRandom() : ((rs = (Math.imul(rs, 1664525) + 1013904223) >>> 0) / 4294967296));
  try {
    const factory = await prepareCourse(id as any, null, false, act, area);
    const world = factory(new THREE.Scene());
    const collision = world.collision;
    const save = new SaveFile(1);
    const starCourses: [string, number][] = [['bob', 1], ['bob', 2], ['bob', 3], ['bob', 4], ['bob', 5], ['bob', 6], ['wf', 1], ['wf', 2],
      ['wf', 3], ['wf', 4], ['wf', 5], ['wf', 6], ['jrb', 1], ['jrb', 2], ['jrb', 3], ['jrb', 4], ['jrb', 5], ['jrb', 6]];
    for (const [c, a] of starCourses) { if (save.stars >= stars) break; save.collectStar(c as any, a); }
    for (let i = 1; save.stars < stars; i++) save.collectStar('ccm', i % 7 + 100 * Math.floor(i / 7) as any);
    const mario = new MarioRuntime(collision, world.spawn, world.deathHeight!);
    const manager = world.objects!.manager;
    const platforms = world.platforms ?? [];
    const goombas = (world.goombas ?? []) as any[];
    const coins = world.coins;
    const redCoins = world.redCoins ?? [];
    const cannons = world.cannons;
    const roomDoors = world.roomDoors as any;
    const signs = world.signs ?? [];
    const counts = { platforms: platforms.length, goombas: goombas.length, coins: coins.length, redCoins: redCoins.length, cannons: (cannons as any)?.entries.length ?? 0,
      doors: roomDoors?.leaves.length ?? 0, signs: signs.length, objects: manager.objects.length };
    let coinsCollected = 0, redCollected = 0;
    let sign: SignReading | undefined;
    let operator: CannonOperator | undefined;
    const camera = new THREE.PerspectiveCamera();
    camera.position.set(0, 1000, 1000);
    const sfx: any = { signMessage() {}, };

    // Feature list for the teleports.
    const paintingSurfaces = new Map<number, Surface[]>();
    for (const s of collision.surfaces) {
      if (s.type >= 0xd3 && s.type < 0xd3 + 0x2d && world.paintings?.[s.type - 0xd3] !== undefined) {
        if (!paintingSurfaces.has(s.type)) paintingSurfaces.set(s.type, []);
        paintingSurfaces.get(s.type)!.push(s);
      }
    }
    const categories = ['platform', 'goomba', 'buddy', 'cannon', 'door', 'sign', 'painting', 'redcoin', 'coin'] as const;
    const listFeatures = (kind: string): Feature[] => {
      switch (kind) {
        case 'platform': return platforms.map(p => ({ kind, pos: (p as any).model.position.toArray() as Vec3, yOffset: 200, spread: 2 }));
        case 'goomba': return goombas.map(g => ({ kind, pos: g.model.position.toArray() as Vec3, yOffset: 120, spread: 3 }));
        case 'buddy': return manager.objects.filter(o => o.behavior === 'bobombBuddy' || o.behavior === 'bobombBuddyAdvice')
          .map(o => ({ kind, pos: [...o.pos] as Vec3, yOffset: 60, spread: 0, frontDist: 55 }));
        case 'cannon': return ((cannons as any)?.entries ?? []).map((c: any) => ({ kind, pos: [...c.home] as Vec3, yOffset: -240, spread: 1 }));
        case 'door': return (roomDoors?.leaves ?? []).map((l: any) => ({ kind, pos: [...l.door.pos] as Vec3, yOffset: 100, spread: 3 }))
          .sort((a: Feature, b: Feature) => a.pos[0] - b.pos[0] || a.pos[1] - b.pos[1] || a.pos[2] - b.pos[2]);
        case 'sign': return signs.map(s => ({ kind, pos: [...s.pos] as Vec3, yOffset: 100, spread: 0, frontYaw: s.yaw, frontDist: 110 }));
        case 'painting': return [...paintingSurfaces.keys()].sort((a, b) => a - b).map(type => {
          const list = paintingSurfaces.get(type)!, s = list[0];
          return { kind, pos: [(s.v1[0] + s.v2[0] + s.v3[0]) / 3, Math.max(s.v1[1], s.v2[1], s.v3[1]), (s.v1[2] + s.v2[2] + s.v3[2]) / 3] as Vec3, yOffset: 200, spread: 0 };
        });
        case 'redcoin': return redCoins.map(c => ({ kind, pos: c.position.toArray() as Vec3, yOffset: 150, spread: 2 }));
        default: return coins.filter((_, i) => i % 3 === 0).map(c => ({ kind, pos: c.position.toArray() as Vec3, yOffset: 150, spread: 3 }));
      }
    };
    const nonEmpty = categories.filter(c => listFeatures(c).length > 0);

    const touches = (pos: THREE.Vector3, radius: number, height: number) => {
      const hb = marioHitbox(mario);
      const [x, y, z] = mario.pos;
      return !(mario.action & ACT_FLAG_INTANGIBLE) && (x - pos.x) ** 2 + (z - pos.z) ** 2 < (radius + hb.radius) ** 2
        && y <= pos.y + height && y + hb.height >= pos.y;
    };
    const addCoins = (value: number) => { coinsCollected += value; mario.healCounter += 4 * value; };
    const collectCoins = (native: boolean): boolean => {
      const px = mario.pos[0], py = mario.pos[1] + 55, pz = mario.pos[2];
      for (const coin of coins) {
        if (!!coin.nativeHitbox !== native || coin.collected || (coin.loot && !coin.loot.tangible)) continue;
        const dx = px - coin.position.x, dy = py - coin.position.y, dz = pz - coin.position.z;
        if (coin.nativeHitbox ? !touches(coin.position, coin.value === 5 ? 120 : 100, 64) : dx * dx + dy * dy + dz * dz > 70 * 70) continue;
        coin.collected = true;
        coin.group.visible = false;
        addCoins(coin.value ?? 1);
        if (coin.nativeHitbox) return true;
      }
      return false;
    };
    const collectRedCoins = (): boolean => {
      for (const coin of redCoins) {
        if (coin.collected || !touches(coin.position, 100, 64)) continue;
        coin.collected = true;
        coin.group.visible = false;
        redCollected++;
        addCoins(2);
        return true;
      }
      return false;
    };
    const nearbySign = () => readableSign(signs, mario, undefined);
    const nearbyBuddy = () => {
      if (!(mario.action & ACT_FLAG_IDLE) && mario.action !== ACT_WALKING) return;
      return manager.objects.find(o => o.active && (o.behavior === 'bobombBuddy' || o.behavior === 'bobombBuddyAdvice')
        && marioOverlapsObject(mario, o) && Math.abs(toS16(o.angleToMario + 0x8000 - mario.faceAngle[1])) <= 0x4000);
    };
    const prevPos = new THREE.Vector3(...mario.pos);

    const miscFields = (): Field[] => {
      const out: Field[] = [coinsCollected, redCollected, mario.healCounter];
      const c = cannons as any;
      if (c) {
        out.push(c.open ? 1 : 0, c.opening);
        for (const e of c.entries) out.push(e.lid.position.x, e.lid.position.y, e.lid.position.z, e.lid.visible ? 1 : 0, e.base.visible ? 1 : 0,
          e.base.position.x, e.base.position.y, e.base.position.z, e.base.rotation.y, e.barrel.rotation.x, e.cooldown, e.rise, e.shake, e.pitch, e.yaw, e.inputYaw);
      }
      if (roomDoors) {
        out.push(roomDoors.passage ? 1 : 0, roomDoors.unlocking ? 1 : 0);
        for (const l of roomDoors.leaves) out.push(l.timer, l.model.position.x, l.model.position.y, l.model.position.z);
      }
      out.push(save.stars, sign ? 1 : 0, operator ? 1 : 0);
      if (sign) { const s = sign as any; out.push(s.phase, s.page, s.clock); }
      if (operator) { const o = operator as any; out.push(o.stage, o.page, o.timer, operator.text ?? ''); }
      out.push(mario.signMotion ? mario.signMotion[0] : 0, mario.signMotion ? mario.signMotion[1] : 0, mario.signMotion ? mario.signMotion[2] : 0);
      const type = mario.floor?.type ?? -1;
      const dest = paintingDestination(type, mario.pos[1] - mario.floorHeight, world.paintings);
      out.push(dest ?? '-');
      return out;
    };

    const next = integerLcg(0xabc12345 + index * 7717);
    const base = atan2s(-64, 0);
    let prevDown = 0;
    let seg: { x: number; y: number; mag: number; down: number; pulse: number; yaw: number; left: number; i: number } | undefined;
    let target: Feature | undefined;
    const frames: number[] = [];
    const dump: unknown[] = [];
    const histogram = new Set<number>();
    const stat = { signFrames: 0, operatorFrames: 0, doorFrames: 0, stomps: 0, hurts: 0, enteredCannon: 0 };
    const featureLog: string[] = [];
    let windowStart = 0, w = -1;
    for (let f = 0; f < GAME_FRAMES; f++) {
      let k = f - windowStart;
      if (f === 0 || (k >= GAME_WINDOW && !sign && !operator) || k >= 4 * GAME_WINDOW) { windowStart = f; k = 0; w++; }
      if (k === 0 && nonEmpty.length) {
        const kind = nonEmpty[w % nonEmpty.length];
        const list = listFeatures(kind);
        target = list[Math.floor(w / nonEmpty.length) % list.length];
        const spread = target.spread ?? 2;
        const rx = next(), rz = next(), rd = next();
        let dx = ((rx % (2 * spread + 1)) - spread) * 70, dz = ((rz % (2 * spread + 1)) - spread) * 70;
        if (target.frontDist !== undefined) {
          const yaw = target.frontYaw ?? (rx % 8) * 0x2000 - 0x8000, dist = target.frontDist + (rd % 3) * 10;
          dx = dist * sins(yaw); dz = dist * coss(yaw);
        }
        mario.pos = [target.pos[0] + dx, target.pos[1] + (target.yOffset ?? 100), target.pos[2] + dz];
        mario.vel = [0, 0, 0];
        mario.forwardVel = 0;
        mario.faceAngle[1] = atan2s(target.pos[2] - mario.pos[2], target.pos[0] - mario.pos[0]);
        mario.setAction(ACT_FREEFALL, 0);
        prevPos.fromArray(mario.pos);
        sign?.dispose(); sign = undefined; operator = undefined;
        seg = undefined;
        featureLog.push(`${w}:${kind}`);
      }
      // Controller.
      let c: { x: number; y: number; mag: number; down: number; yaw: number };
      if (sign || operator) {
        const d = f % 5 === 0 ? A_BUTTON : 0;
        c = { x: 0, y: 0, mag: 0, down: d, yaw: 0 };
      } else if (target && (target.kind === 'sign' || target.kind === 'buddy') && k >= 24 && k < 110) {
        // Stand still and tap A: reads the sign / talks to the buddy.
        c = { x: 0, y: 0, mag: 0, down: k % 8 === 0 ? A_BUTTON : 0, yaw: 0 };
      } else if (k < 45 && target) {
        const t = target.pos;
        const want = atan2s(t[2] - mario.pos[2], t[0] - mario.pos[0]);
        const d = k % 20 === 5 || k % 20 === 6 || k % 20 === 10 ? A_BUTTON : k % 20 === 15 ? B_BUTTON : 0;
        c = target.kind === 'sign' ? { x: 0, y: 0, mag: 0, down: 0, yaw: 0 } : { x: 0, y: 64, mag: 64, down: d, yaw: toS16(want - base) };
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
        if (k % 14 === 0) d |= B_BUTTON;
        seg.i++; seg.left--;
        c = { x: seg.x, y: seg.y, mag: seg.mag, down: d, yaw: seg.yaw };
      }
      const controller: ControllerSnapshot = { stickX: c.x, stickY: c.y, stickMag: c.mag, buttonDown: c.down, buttonPressed: c.down & ~prevDown };
      prevDown = c.down;

      // ---- Game.stepWorld (the simulation part) ----
      prevPos.fromArray(mario.pos);
      let finished = false;
      if (!sign && (controller.buttonPressed & (A_BUTTON | B_BUTTON))) {
        const s = nearbySign();
        if (s) sign = new SignReading(s, mario, camera, sfx);
      }
      if (sign) {
        stat.signFrames++;
        mario.step(controller, c.yaw);
        if (sign.step(mario, controller, camera)) { sign.dispose(); sign = undefined; }
        finished = true;
      } else if (operator && cannons) {
        stat.operatorFrames++;
        const op = operator;
        const done = op.step(mario, controller, camera, cannons, () => save.openCannon(id as any), () => {});
        if (mario.action === ACT_READING_NPC_DIALOG) mario.step(controller, c.yaw);
        world.objects?.afterMarioStep(mario);
        if (done) operator = undefined;
        finished = true;
      } else if (roomDoors?.step(mario, save, () => {})) {
        stat.doorFrames++;
        finished = true;
      }
      if (!finished) {
        for (const platform of platforms) platform.step(mario);
        for (const enemy of goombas) {
          if (enemy.step(mario.pos, collision)) world.objects?.spawnCoin(enemy.model.position.toArray() as Vec3, true);
        }
        if (mario.invincTimer > 0) mario.invincTimer--;
        const placedCoin = collectCoins(true) || collectRedCoins();
        const events = world.objects?.step(mario, undefined, placedCoin) ?? [];
        for (const event of events) if (event.type === 'movingCoinCollected') addCoins(1);
        let stopped = false;
        if (cannons) {
          if (controller.buttonPressed & (A_BUTTON | B_BUTTON)) {
            const buddy = nearbyBuddy();
            if (buddy) { operator = new CannonOperator(buddy, id === 'bob', save.cannonOpen(id as any)); stopped = true; }
          }
          if (!stopped) cannons.step(mario);
        }
        if (!stopped) {
          mario.step(controller, c.yaw);
          world.objects?.afterMarioStep(mario);
          if (!mario.events.some(e => e.type === 'respawn')) {
            for (const enemy of goombas as GoombaEnemy[]) {
              if (!enemy.tangible) continue;
              const model = (enemy as any).model as THREE.Group;
              const descendingOntoBody = (mario.action & ACT_FLAG_AIR) && mario.vel[1] < 0
                && mario.pos[1] > model.position.y && mario.pos[1] <= enemy.top
                && Math.hypot(mario.pos[0] - model.position.x, mario.pos[2] - model.position.z) < enemy.radius + 37;
              if (!descendingOntoBody && !crossesGoomba(prevPos, mario.pos, enemy)) {
                const [x, y, z] = mario.pos;
                const foot = model.position;
                if (y <= enemy.top && y + marioHitbox(mario).height >= foot.y
                    && Math.hypot(x - foot.x, z - foot.z) < enemy.radius + 37 + swordReach(mario)) {
                  const angle = atan2s(foot.z - z, foot.x - x);
                  const difference = Math.abs(toS16(angle - mario.faceAngle[1]));
                  const strike = mario.flags & (MARIO_PUNCHING | MARIO_KICKING | MARIO_TRIPPING);
                  if ((strike && difference <= 0x2aaa) || ((mario.action & ACT_FLAG_ATTACKING) && Math.abs(mario.forwardVel) >= 26)) {
                    world.objects?.spawnCoin(foot.toArray() as Vec3, true);
                    enemy.defeat();
                    continue;
                  }
                  if ((mario.action & (ACT_FLAG_INTANGIBLE | ACT_FLAG_INVULNERABLE)) || mario.invincTimer
                      || !touchesGoombaHurtbox(mario.pos, marioHitbox(mario).height, enemy)) continue;
                  const hurt = mario.takeDamage(1, [foot.x, foot.y, foot.z]);
                  enemy.alert(x, z);
                  if (hurt) { stat.hurts++; break; }
                }
                continue;
              }
              enemy.defeat(true);
              stat.stomps++;
              if (mario.action !== ACT_GROUND_POUND && mario.action !== ACT_GROUND_POUND_LAND) {
                mario.pos[1] = enemy.top;
                if (!(mario.action & ACT_FLAG_AIR)) mario.setAction(ACT_FREEFALL);
                mario.vel[1] = 30;
                mario.flags &= ~MARIO_UNKNOWN_08;
              }
              break;
            }
          }
          collectCoins(false);
        }
      }
      histogram.add(mario.action);
      if (process.env.GAME_SURF && f === 0) {
      writeFileSync(process.env.GAME_SURF + '.obj', JSON.stringify(manager.objects.map(o => [o.behavior, o.active, o.action, o.param, ...o.pos])));
      writeFileSync(process.env.GAME_SURF, JSON.stringify((collision.surfaces as Surface[]).map(s => [...s.v1, ...s.v2, ...s.v3, s.type, s.force, s.flags, s.room ?? -1, s.object ? manager.objects.indexOf(s.object as any) : -1]),
        (_k, v) => (Object.is(v, -0) ? '-0' : v)));
    }
      if (mario.action === ACT_IN_CANNON) stat.enteredCannon++;

      // ---- hashes ----
      const objects = manager.objects as any[];
      const hMario = hashFields(marioFields(mario, objects));
      const hObjects = new Hasher();
      hObjects.word(objects.length);
      for (const o of objects) hashFields(objectFields(o), hObjects);
      const hPlat = new Hasher(); platforms.forEach(p => hashFields(platformFields(p), hPlat));
      const hGoomba = new Hasher(); goombas.forEach(g => hashFields(goombaFields(g), hGoomba));
      const hCoin = new Hasher(); hCoin.word(coins.length);
      coins.forEach(coin => hashFields(coinFields(coin), hCoin)); redCoins.forEach(coin => hashFields(coinFields(coin), hCoin));
      const hWorld = worldHash(collision, objects);
      const hMisc = hashFields(miscFields());
      frames.push(hMario.h, hObjects.h, hPlat.h, hGoomba.h, hCoin.h, hWorld, hMisc.h);
      if (detail) {
        dump.push({
          mario: marioFields(mario, objects), objects: objects.map(objectFields), platforms: platforms.map(platformFields),
          goombas: goombas.map(goombaFields), coins: [...coins, ...redCoins].map(coinFields), misc: miscFields(),
        });
      }
    }
    return {
      id, act, area, stars, frames, actions: histogram.size,
      counts,
      collected: { ...stat, coins: coinsCollected, red: redCollected, cannonOpen: cannons ? (cannons as any).open : false, stars: save.stars },
      features: nonEmpty,
      dump,
    };
  } finally { Math.random = realRandom; }
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust app-layer game fixtures', () => {
  it('writes game goldens', async () => {
    installStubs();
    const only = process.env.GAME_DETAIL;
    const out: unknown[] = [];
    for (const [i, [id, act, area, stars]] of GAME_COURSES.entries()) {
      const tag = `${id}.${act}.${area}`;
      if (process.env.GAME_ONLY && process.env.GAME_ONLY !== tag) { out.push({ id, act, area, skipped: true }); continue; }
      const r = await runGame(id, act, area, stars, i, only === tag);
      console.log(tag, JSON.stringify(r.counts), 'actions', r.actions, JSON.stringify(r.collected), r.features.join(','));
      if (only === tag) writeFileSync(process.env.GAME_OUT!, JSON.stringify(r.dump, (_k, v) => (Object.is(v, -0) ? '-0' : v)));
      out.push({ ...r, dump: undefined });
    }
    mkdirSync('native/sim/tests/fixtures', { recursive: true });
    if (!process.env.GAME_ONLY) writeFileSync('native/sim/tests/fixtures/game.json', JSON.stringify({ frames: GAME_FRAMES, window: GAME_WINDOW, hashes: 7, courses: out }));
  }, 900000);
});
