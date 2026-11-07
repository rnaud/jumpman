// Second app-layer differential generator for the Rust port: the King Bob-omb dialog (ids 17/116/128), Koopa the Quick
// and the racing penguin questions, the Wing Cap switch, CoinEffects, fading and instant warps. Copies the matching parts of
// `Game.stepWorld` (see rustFixtures.game.gen.test.ts) and writes per-frame state hashes to native/sim/tests/fixtures/game2.json.
// `GEN_RUST_FIXTURES=1 npx vitest run tests/rustFixtures.game2.gen.test.ts`; compared by native/src/game/game2_tests.rs.
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
// Scenarios. Each returns { name, frames: number[] (hashes per frame), stats }.
// ================================================================================================
export const G2_FRAMES = 900;
const DIALOG_IDS = [17, 116, 128];

function integerLcg(seed: number) {
  let s = seed >>> 0;
  return () => ((s = (Math.imul(s, 1664525) + 1013904223) >>> 0) >>> 16);
}

async function setup(id: string, act: number, area: number, index: number) {
  let rs = (0x2badbeef + index * 7919) >>> 0;
  const realRandom = Math.random;
  Math.random = () => (new Error().stack!.includes('generateUUID') ? realRandom() : ((rs = (Math.imul(rs, 1664525) + 1013904223) >>> 0) / 4294967296));
  const factory = await prepareCourse(id as any, null, false, act, area);
  const world = factory(new THREE.Scene());
  const save = new SaveFile(1);
  const mario = new MarioRuntime(world.collision, world.spawn, world.deathHeight!);
  return { world, save, mario, manager: world.objects!.manager, restore: () => { Math.random = realRandom; } };
}

function c(x: number, y: number, down: number, prev: number): ControllerSnapshot {
  return { stickX: x, stickY: y, stickMag: Math.min(64, Math.sqrt(x * x + y * y)), buttonDown: down, buttonPressed: down & ~prev };
}

const vec = (v: THREE.Vector3) => [v.x, v.y, v.z];

function allObjects(objects: any[]): number {
  const all = new Hasher();
  all.word(objects.length);
  for (const o of objects) hashFields(objectFields(o), all);
  return all.h;
}

// ---- King Bob-omb dialog (Game.stepWorld kingDialog block) --------------------------------------
async function kingScenario(dialogId: number, index: number) {
  const { world, mario, manager, restore } = await setup('bob', 1, 1, index);
  try {
    const king = manager.objects.find(o => o.behavior === 'kingBobomb')!;
    king.vars.dialog = dialogId;
    const objects = manager.objects as any[];
    mario.pos = [king.pos[0] + 300, king.pos[1] + 40, king.pos[2]];
    mario.faceAngle[1] = atan2s(king.pos[2] - mario.pos[2], king.pos[0] - mario.pos[0]);
    mario.setAction(ACT_FREEFALL, 0);
    const camera = new THREE.PerspectiveCamera();
    camera.position.set(Math.round(mario.pos[0]) + 500, Math.round(mario.pos[1]) + 400, Math.round(mario.pos[2]) + 500);
    const savedFocus = new THREE.Vector3(Math.round(mario.pos[0]), Math.round(mario.pos[1]) + 100, Math.round(mario.pos[2]));
    let dialog: { king: any; id: number; page: number } | undefined;
    let kingCamera: NpcDialogCamera | undefined;
    const log: Field[] = [];
    let shown = '';
    const frames: number[] = [];
    let prevDown = 0, dialogs = 0, completed = 0, maxPage = 0;
    const next = integerLcg(77 + index);
    for (let f = 0; f < G2_FRAMES; f++) {
      let down = f > 30 && f % 9 === 0 ? A_BUTTON : f % 31 === 7 ? B_BUTTON : 0;
      if (next() % 40 === 0) down = A_BUTTON;
      const controller = c(0, 0, down, prevDown);
      prevDown = down;
      const yaw = 0;
      if (kingCamera?.returnFrames) {
        kingCamera.stepReturn(camera, world.collision);
        if (!kingCamera.returnFrames) kingCamera = undefined;
      }
      if (dialog) {
        const k = dialog.king;
        if (kingCamera?.returnFrames !== undefined) {
          if (kingCamera.returnFrames === 3) { mario.actionState = 9; k.vars.dialogDone = 1; dialog = undefined; completed++; }
        } else {
          if (dialog.id !== 116) { k.turnTowardAngle(k.angleToMario, 0x800); k.faceYaw = k.moveYaw; }
          mario.step(controller, yaw);
          if (mario.actionState === 8 && (dialog.id === 116 || k.moveYaw === k.angleToMario)) {
            const p = kingDialogPages[dialog.id];
            kingCamera ??= new NpcDialogCamera(camera, savedFocus.clone(), mario.pos, k.pos, k.hitbox.height);
            kingCamera.step(camera, mario.pos);
            if (dialog.page < 0) {
              if (kingCamera.frame === 11) { shown = p[++dialog.page]; }
            } else if (controller.buttonPressed & (A_BUTTON | B_BUTTON)) {
              shown = p[++dialog.page] ?? '';
              maxPage = Math.max(maxPage, dialog.page);
              if (dialog.page === p.length) {
                kingCamera.beginReturn(camera);
                log.push(dialog.id === 116 ? 'kingDefeat' : 'kingFight');
                dialog.page = -1;
              }
            }
          }
        }
      } else {
        if (mario.invincTimer > 0) mario.invincTimer--;
        const events = world.objects!.step(mario, undefined, false);
        mario.step(controller, yaw);
        world.objects!.afterMarioStep(mario);
        for (const e of events) {
          if (e.type === 'kingDialog') {
            dialog = { king: e.king, id: e.id, page: -1 };
            mario.usedObj = e.king;
            mario.setAction(ACT_READING_NPC_DIALOG, 2);
            dialogs++;
            log.push('begin', e.id);
          } else if (e.type === 'boss' || e.type === 'kingLeave') log.push(e.type);
        }
      }
      const dh = new Hasher();
      hashFields([dialog ? 1 : 0, dialog ? dialog.page : -9, kingCamera ? kingCamera.frame : -1, kingCamera?.returnFrames ?? -1,
        ...(kingCamera ? [...vec(camera.position), ...vec((kingCamera as any).focus)] : []), shown, ...log], dh);
      frames.push(hashFields(marioFields(mario, objects)).h, hashFields(objectFields(king)).h, dh.h);
      if (process.env.GAME2_DUMP === 'king' + dialogId) console.log('DUMP', f, JSON.stringify([dialog ? 1 : 0, dialog ? dialog.page : -9, kingCamera ? kingCamera.frame : -1, kingCamera?.returnFrames ?? -1,
        ...(kingCamera ? [...vec(camera.position), ...vec((kingCamera as any).focus)] : []), shown, ...log]));
    }
    return { name: 'king' + dialogId, frames, groups: 3, stats: { dialogs, completed, maxPage } };
  } finally { restore(); }
}

// ---- Koopa the Quick (bob act 2) and the racing penguin (ccm act 2) ----------------------------------
async function raceScenario(kind: 'koopaA' | 'koopaB' | 'penguinA' | 'penguinB', index: number) {
  const koopaKind = kind.startsWith('koopa');
  let setupResult = await setup(koopaKind ? 'bob' : 'ccm', 1, koopaKind ? 1 : 2, index);
  let raceAct = 1;
  while (!setupResult.manager.findByBehavior(koopaKind ? 'koopaTheQuick' : 'racingPenguin') && raceAct < 6) {
    setupResult.restore();
    setupResult = await setup(koopaKind ? 'bob' : 'ccm', ++raceAct, koopaKind ? 1 : 2, index);
  }
  const { world, mario, manager, restore } = setupResult;
  try {
    const objects = manager.objects as any[];
    const target = manager.findByBehavior(koopaKind ? 'koopaTheQuick' : 'racingPenguin')!;
    const save = new SaveFile(1);
    const coins = world.coins, redCoins = world.redCoins ?? [];
    let coinsCollected = 0;
    const addCoins = (value: number) => { coinsCollected += value; mario.healCounter += 4 * value; };
    const touches = (pos: THREE.Vector3, radius: number, height: number) => {
      const hb = marioHitbox(mario); const [x, y, z] = mario.pos;
      return !(mario.action & ACT_FLAG_INTANGIBLE) && (x - pos.x) ** 2 + (z - pos.z) ** 2 < (radius + hb.radius) ** 2
        && y <= pos.y + height && y + hb.height >= pos.y;
    };
    const collectCoins = (native: boolean): boolean => {
      const px = mario.pos[0], py = mario.pos[1] + 55, pz = mario.pos[2];
      for (const coin of coins) {
        if (!!coin.nativeHitbox !== native || coin.collected || (coin.loot && !coin.loot.tangible)) continue;
        const dx = px - coin.position.x, dy = py - coin.position.y, dz = pz - coin.position.z;
        if (coin.nativeHitbox ? !touches(coin.position, coin.value === 5 ? 120 : 100, 64) : dx * dx + dy * dy + dz * dz > 70 * 70) continue;
        coin.collected = true; coin.group.visible = false; addCoins(coin.value ?? 1);
        if (coin.nativeHitbox) return true;
      }
      return false;
    };
    const collectRedCoins = (): boolean => {
      for (const coin of redCoins) {
        if (coin.collected || !touches(coin.position, 100, 64)) continue;
        coin.collected = true; coin.group.visible = false; addCoins(2);
        return true;
      }
      return false;
    };
    mario.pos = [target.pos[0] + (koopaKind ? 250 : 150), target.pos[1] + (koopaKind ? 30 : 20), target.pos[2]];
    mario.faceAngle[1] = atan2s(target.pos[2] - mario.pos[2], target.pos[0] - mario.pos[0]);
    mario.setAction(ACT_FREEFALL, 0);
    const frames: number[] = [];
    const next = integerLcg(1234 + index);
    let prevDown = 0, held = 0, questions = 0, raceEvents = 0;
    const accept = kind.endsWith('A');
    const KOOPA_Q = [KoopaRace.Invitation, KoopaRace.Countdown, KoopaRace.ResultDialog];
    const eventTag = (e: any) => e.type + (e.state ?? e.text ?? e.cue ?? '');
    for (let f = 0; f < G2_FRAMES + 600; f++) {
      let down = 0, sx = 0, sy = 0;
      const koopa = koopaKind ? target : undefined;
      const racer = koopaKind ? undefined : target;
      const asking = koopa ? KOOPA_Q.includes(koopa.action) : racer!.action === 1;
      if (asking) {
        held++;
        if (held > 6 && held % 5 === 1) down = accept ? A_BUTTON : B_BUTTON;
      } else {
        held = 0;
        const mag = next() % 3 === 0 ? 0 : 64;
        sx = mag ? (next() % 129) - 64 : 0; sy = mag ? (next() % 129) - 64 : 0;
        if (next() % 5 === 0) down |= A_BUTTON;
        if (next() % 9 === 0) down |= B_BUTTON;
      }
      const controller = c(sx, sy, down, prevDown);
      prevDown = down;
      const evs: string[] = [];
      if (koopa && KOOPA_Q.includes(koopa.action)) {
        questions++;
        koopa.vars.answer = controller.buttonPressed & A_BUTTON ? 1 : controller.buttonPressed & B_BUTTON ? -1 : 0;
        for (const e of world.objects!.step(mario, koopa)) evs.push(eventTag(e));
        koopa.vars.answer = 0;
      } else {
        if (mario.invincTimer > 0) mario.invincTimer--;
        if (racer) {
          racer.scale = save.stars === 120 ? 8 : 4;
          if (racer.action === 1) racer.vars.answer = controller.buttonPressed & A_BUTTON ? 1 : controller.buttonPressed & B_BUTTON ? -1 : 0;
        }
        const placedCoin = collectCoins(true) || collectRedCoins();
        const events = world.objects!.step(mario, undefined, placedCoin);
        for (const e of events) if (e.type === 'movingCoinCollected') addCoins(1);
        for (const e of events) if (e.type !== 'movingCoinCollected') evs.push(eventTag(e));
        let stopped = false;
        if (koopa && (koopa.action === KoopaRace.Invitation || koopa.action === KoopaRace.ResultDialog)) stopped = true;
        else if (racer && (racer.action === 1 || racer.action === 2 || racer.vars.answer)) { racer.vars.answer = 0; stopped = true; }
        if (!stopped) {
          mario.step(controller, 0);
          world.objects!.afterMarioStep(mario);
          collectCoins(false);
        }
      }
      for (const e of evs) if (e.startsWith('race')) raceEvents++;
      frames.push(hashFields(marioFields(mario, objects)).h, hashFields(objectFields(target)).h,
        hashFields([...evs, target.action, coinsCollected]).h, allObjects(objects), worldHash(world.collision, objects));
      if (process.env.GAME2_DUMP === kind && f === Number(process.env.GAME2_FRAME ?? 0)) console.log('DUMPTARGET', JSON.stringify(objectFields(target)), '\nDUMPMARIO', JSON.stringify(marioFields(mario, objects)),'\nDUMPOBJ', JSON.stringify(objects.map(o => [o.behavior, hashFields(objectFields(o)).h])), JSON.stringify(evs));
    }
    return { name: kind, act: raceAct, frames, groups: 5, stats: { questions, raceEvents, coins: coinsCollected, targetAction: target.action } };
  } finally { restore(); }
}

// ---- Tower of the Wing Cap switch ------------------------------------------------------------------
async function wingScenario(index: number) {
  const { world, mario, manager, restore, save } = await setup('totwc', 1, 1, index);
  try {
    const sw = world.wingSwitch as any;
    const objects = manager.objects as any[];
    const top = sw.pos as Vec3;
    const frames: number[] = [];
    const next = integerLcg(99);
    let presses = 0, prevDown = 0;
    for (let f = 0; f < 420; f++) {
      if (f % 140 === 0) {
        mario.pos = [top[0] + ((next() % 5) - 2) * 20, top[1] + 150 + (f % 3) * 40, top[2] + ((next() % 5) - 2) * 20];
        mario.vel = [0, 0, 0]; mario.forwardVel = 0;
        mario.setAction(ACT_FREEFALL, 0);
        if (f === 280) { sw.reset(false); world.objects!.setWingBoxes(false); }
      }
      const down = f % 37 === 3 ? A_BUTTON : 0;
      const controller = c(0, 0, down, prevDown);
      prevDown = down;
      world.objects!.step(mario, undefined, false);
      mario.step(controller, 0);
      world.objects!.afterMarioStep(mario);
      const pressed = sw.step(mario);
      if (pressed) { presses++; save.pressWingSwitch(); world.objects!.setWingBoxes(true); }
      frames.push(hashFields(marioFields(mario, objects)).h, hashFields([sw.timer, sw.height, pressed ? 1 : 0, save.wingSwitchOn ? 1 : 0]).h,
        worldHash(world.collision, objects), allObjects(objects));
    }
    return { name: 'wing', frames, groups: 4, stats: { presses } };
  } finally { restore(); }
}

// ---- CoinEffects (positions that use Math.random are excluded) --------------------------------------------
function coinFxScenario() {
  const root = new THREE.Group();
  const fx = new CoinEffects(root, () => new THREE.Group());
  const next = integerLcg(4242);
  const frames: number[] = [];
  const snap = (): number => {
    const f = fx as any;
    const digits = f.digits.map((d: any) => [d.model.position.y, d.age, d.velocity].join(',')).sort();
    const bursts = f.bursts.map((b: any) => b.left).sort();
    const sparks = f.sparks.map((s: any) => [s.age, s.group.scale.x, s.group.children.findIndex((n: any) => n.visible)].join(',')).sort();
    return hashFields([digits.length, ...digits, bursts.length, ...bursts, sparks.length, ...sparks]).h;
  };
  let spawned = 0;
  for (let t = 0; t < 400; t++) {
    if (next() % 9 === 0) { fx.burst(new THREE.Vector3(1, 2, 3)); spawned++; }
    if (next() % 21 === 0) { fx.number(1 + (next() % 7), new THREE.Vector3(10, 20 + (next() % 5), 30)); spawned++; }
    fx.step();
    frames.push(snap());
  }
  fx.clear();
  frames.push(snap());
  return { name: 'coinfx', frames, groups: 1, stats: { spawned } };
}

// ---- Fading warps (bob) ----------------------------------------------------------------------------------
async function fadeScenario(index: number) {
  const { world, mario, manager, restore } = await setup('bob', 1, 1, index);
  try {
    const pads = world.fadingWarps!;
    const objects = manager.objects as any[];
    const frames: number[] = [];
    let fadingWarp: (typeof pads)[number] | undefined, justTeleported = false;
    let opacity = 0, teleports = 0, prevDown = 0;
    const prevPos = new THREE.Vector3(...mario.pos);
    let prevYaw = 0;
    const cues: Field[] = [];
    for (let f = 0; f < 1040; f++) {
      if (f % 260 === 0) {
        const pad = pads[(f / 260) % pads.length];
        mario.pos = [pad.pos[0] + (f % 520 ? 30 : 0), pad.pos[1] + 120, pad.pos[2]];
        mario.vel = [0, 0, 0]; mario.forwardVel = 0;
        mario.setAction(ACT_FREEFALL, 0);
      }
      const down = (f % 260 > 70 && f % 260 < 90 ? Z_TRIG : 0) | (f % 61 === 0 ? A_BUTTON : 0);
      const controller = c(0, 0, down, prevDown);
      prevDown = down;
      prevPos.fromArray(mario.pos);
      // stepFadingWarp
      const m = mario;
      if (m.action === ACT_TELEPORT_FADE_OUT && fadingWarp) {
        const t = m.actionTimer;
        opacity = Math.max(0, (t - 20) / 20);
        if (t >= 40) {
          const { target, faceYaw } = fadingWarp;
          m.pos = [...target]; m.vel = [0, 0, 0]; m.forwardVel = 0; m.faceAngle = [0, faceYaw, 0]; m.peakHeight = target[1];
          m.setAction(ACT_TELEPORT_FADE_IN);
          prevPos.fromArray(target); prevYaw = faceYaw; cues.push('teleport');
          fadingWarp = undefined; teleports++;
        }
      } else if (m.action === ACT_TELEPORT_FADE_IN) {
        opacity = Math.max(0, 1 - m.actionTimer / 20);
      } else {
        const hit = marioHitbox(m);
        const pad = pads.find(({ pos: [x, y, z] }) => Math.hypot(m.pos[0] - x, m.pos[2] - z) < 85 + hit.radius && m.pos[1] <= y + 50 && m.pos[1] + hit.height >= y);
        if (!pad) justTeleported = false;
        else if (!justTeleported && (m.action === ACT_IDLE || m.action === ACT_CROUCHING)) {
          justTeleported = true; fadingWarp = pad; cues.push('teleport');
          m.setAction(ACT_TELEPORT_FADE_OUT, Number(m.action === ACT_CROUCHING));
        }
      }
      if (mario.invincTimer > 0) mario.invincTimer--;
      world.objects!.step(mario, undefined, false);
      mario.step(controller, 0);
      world.objects!.afterMarioStep(mario);
      frames.push(hashFields(marioFields(mario, objects)).h, hashFields([Math.fround(opacity), justTeleported ? 1 : 0, fadingWarp ? 1 : 0, prevYaw, cues.length]).h);
      if (process.env.GAME2_DUMP === 'fade' && f >= 170 && f < 178) console.log('DUMPF', f, JSON.stringify([Math.fround(opacity), justTeleported ? 1 : 0, fadingWarp ? 1 : 0, prevYaw, cues.length, mario.action, mario.actionTimer]));
    }
    return { name: 'fade', frames, groups: 2, stats: { teleports } };
  } finally { restore(); }
}

// ---- Instant warps (castle_inside area 2 shifts Mario in place) -------------------------------------------
async function instantScenario(index: number) {
  const { world, mario, manager, restore } = await setup('castle_inside', 1, 2, index);
  try {
    const objects = manager.objects as any[];
    const warps = (world as any).instantWarps as Record<number, { area: number; delta: Vec3 }>;
    const surfaces = (world.collision.surfaces as Surface[]).filter(s => s.type >= 0x1b && warps[s.type - 0x1b]);
    const frames: number[] = [];
    const prevPos = new THREE.Vector3(...mario.pos);
    let shifts = 0, hits = 0;
    for (let f = 0; f < 360; f++) {
      // Game.stepWorld applies the shift first (from the floor of the previous step)...
      const shift = warps[(mario.floor?.type ?? 0) - 0x1b];
      if (shift && shift.area === 2) {
        for (let axis = 0; axis < 3; axis++) mario.pos[axis] += shift.delta[axis];
        prevPos.fromArray(mario.pos); shifts++;
      }
      // ...the script's teleport comes after.
      if (f % 90 === 0 && surfaces.length) {
        const s = surfaces[((f / 90) * 7) % surfaces.length];
        mario.pos = [(s.v1[0] + s.v2[0] + s.v3[0]) / 3, Math.max(s.v1[1], s.v2[1], s.v3[1]) + 60, (s.v1[2] + s.v2[2] + s.v3[2]) / 3];
        mario.vel = [0, 0, 0]; mario.forwardVel = 0; mario.setAction(ACT_FREEFALL, 0);
        hits++;
      }
      world.objects!.step(mario, undefined, false);
      mario.step(c(0, 0, 0, 0), 0);
      world.objects!.afterMarioStep(mario);
      frames.push(hashFields(marioFields(mario, objects)).h, hashFields([shifts]).h);
    }
    return { name: 'instant', frames, groups: 2, stats: { shifts, hits, surfaces: surfaces.length } };
  } finally { restore(); }
}

describe.skipIf(!process.env.GEN_RUST_FIXTURES)('rust app-layer game fixtures 2', () => {
  it('writes game2 goldens', async () => {
    installStubs();
    const out: any[] = [];
    let i = 0;
    for (const id of DIALOG_IDS) out.push(await kingScenario(id, i++));
    for (const k of ['koopaA', 'koopaB', 'penguinA', 'penguinB'] as const) out.push(await raceScenario(k, i++));
    out.push(await wingScenario(i++));
    out.push(coinFxScenario());
    out.push(await fadeScenario(i++));
    out.push(await instantScenario(i++));
    for (const r of out) console.log(r.name, r.frames.length, JSON.stringify(r.stats));
    mkdirSync('native/sim/tests/fixtures', { recursive: true });
    writeFileSync('native/sim/tests/fixtures/game2.json', JSON.stringify({ scenarios: out }));
  }, 900000);
});
