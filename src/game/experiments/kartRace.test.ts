import { existsSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { CollisionWorld, type WallCollisionData } from '../../sim/collision';
import { SURFACE_FLAG_X_PROJECTION, SURFACE_NOT_SLIPPERY } from '../../sim/surfaces';
import { KartPhysics, type KartGround, type KartInput } from './kartPhysics';
import {
  BOOST_TICKS, COUNTDOWN_TICKS, GATES, GATE_HALF, KartRace, LAPS, RACERS, ROULETTE_TICKS, Rng, SHELL_LIFE, SPIN_TICKS, formatTicks,
  gateCrossed, type Gate, type Shell,
} from './kartRace';

const flat = (wallX?: number): KartGround => ({
  floor: () => ({ height: 0, normal: [0, 1, 0], grass: false }),
  wall: pos => (wallX !== undefined && pos[0] > wallX ? { pos: [wallX - 2, pos[1], pos[2]], hits: [{ normal: [-1, 0, 0], xFacing: true }] } : null),
});
/** A 1200 x 800 rectangle that starts mid-edge heading -z like Luigi Raceway, plus the export's sentinel node. */
function oval(): number[][] {
  const pts: number[][] = [];
  const corners = [[0, 0], [0, -400], [1200, -400], [1200, 400], [0, 400]];
  for (let k = 0; k < corners.length; k++) {
    const a = corners[k], b = corners[(k + 1) % corners.length];
    const n = Math.trunc((Math.abs(b[0] - a[0]) + Math.abs(b[1] - a[1])) / 20);
    for (let s = 0; s < n; s++) pts.push([a[0] + (b[0] - a[0]) * s / n, 0, a[1] + (b[1] - a[1]) * s / n]);
  }
  pts.push([-32768, -32768, -32768]);
  return pts;
}
const PAD: KartInput = { rawX: 0, rawY: 0, accel: false, brake: false, hopDown: false, hopPressed: false };
const GO: KartInput = { ...PAD, rawY: 80, accel: true };
function race(seed = 7): { r: KartRace; k: KartPhysics } {
  const r = new KartRace(oval(), seed);
  const k = r.start(flat());
  return { r, k };
}
const fwd = (k: KartPhysics): [number, number] => [Math.sin(k.yaw()), Math.cos(k.yaw())];

describe('kart race: checkpoints and laps', () => {
  it('drops the sentinel node and orders the gates', () => {
    const { r } = race();
    expect(r.track.nodes.every(p => p[0] > -30000)).toBe(true);
    expect(r.track.gates).toHaveLength(GATES);
    for (let i = 1; i < GATES; i++) expect(r.track.gates[i].arc).toBeGreaterThan(r.track.gates[i - 1].arc);
    expect(Math.abs(r.track.length - 4000)).toBeLessThan(60);
  });
  it('counts only forward crossings inside the gate width', () => {
    const g: Gate = { c: [0, 0], t: [0, -1], arc: 0 };
    expect(gateCrossed(g, [0, 5], [0, -5])).toBe(true);
    expect(gateCrossed(g, [0, -5], [0, 5])).toBe(false);
    expect(gateCrossed(g, [GATE_HALF + 50, 5], [GATE_HALF + 50, -5])).toBe(false);
    expect(gateCrossed(g, [GATE_HALF - 5, 5], [GATE_HALF - 5, -5])).toBe(true);
    expect(gateCrossed(g, [0, 5], [0, 2])).toBe(false);
  });
  it('requires gates in order: skipping and reversing do not count', () => {
    const { r } = race();
    r.phase = 'racing';
    const t = r.track;
    const before = (g: number): [number, number] => [t.gates[g].c[0] - t.gates[g].t[0] * 15, t.gates[g].c[1] - t.gates[g].t[1] * 15];
    const after = (g: number): [number, number] => [t.gates[g].c[0] + t.gates[g].t[0] * 15, t.gates[g].c[1] + t.gates[g].t[1] * 15];
    r.racers[1].last = before(0); r.progressStep(1, after(0));
    expect(r.racers[1].passed).toBe(1);
    r.racers[1].last = before(2); r.progressStep(1, after(2));
    expect(r.racers[1].passed).toBe(1);
    r.racers[1].last = after(1); r.progressStep(1, before(1));
    expect(r.racers[1].passed).toBe(1);
    r.racers[1].last = before(1); r.progressStep(1, after(1));
    expect(r.racers[1].passed).toBe(2);
  });
  it('counts laps and finishes at three', () => {
    const { r } = race();
    r.phase = 'racing';
    let tick = 0;
    for (let lap = 0; lap <= LAPS; lap++) {
      for (let g = 0; g < GATES; g++) {
        tick += 10;
        r.raceTick = tick;
        const { c, t } = r.track.gates[g];
        r.racers[0].last = [c[0] - t[0] * 10, c[1] - t[1] * 10];
        r.progressStep(0, [c[0] + t[0] * 10, c[1] + t[1] * 10]);
      }
    }
    expect(r.racers[0].laps).toHaveLength(LAPS);
    expect(r.racers[0].passed).toBe(GATES * (LAPS + 1));
    expect(r.phase).toBe('finished');
    expect(r.results!.laps[0]).toBe(GATES * 10);
    expect(r.bestLap).toBe(GATES * 10);
    expect(r.results!.newBestTotal).toBe(true);
  });
  it('holds the karts through 3-2-1 and releases them at GO', () => {
    const { r, k } = race();
    const g = flat();
    const z0 = k.pos[2];
    const events = [];
    for (let i = 0; i < COUNTDOWN_TICKS - 1; i++) { r.tick(GO, false, k, g); events.push(...r.drainEvents()); }
    expect(r.phase).toBe('countdown');
    expect(Math.abs(k.pos[2] - z0)).toBeLessThan(0.5);
    expect(r.banner()).toBe(1);
    r.tick(GO, false, k, g);
    expect(r.phase).toBe('racing');
    expect(r.drainEvents()).toContainEqual({ kind: 'go' });
    expect(events).toContainEqual({ kind: 'count', n: 2 });
    expect(events).toContainEqual({ kind: 'count', n: 1 });
    for (let i = 0; i < 60; i++) r.tick(GO, false, k, g);
    expect(k.pos[2]).toBeLessThan(z0 - 30);
  });
  it('falling off puts the player back on the last gate and keeps progress', () => {
    const { r, k } = race();
    r.phase = 'racing';
    r.racers[0].passed = 3; r.racers[0].nextGate = 3;
    (r as { fallY: number }).fallY = 100; // the flat floor at 0 is now "below the world"
    const p = r.tick(PAD, false, k, flat());
    expect(r.racers[0].rescues).toBe(1);
    const gate = r.track.gates[2];
    expect(Math.hypot(p.pos[0] - gate.c[0], p.pos[2] - gate.c[1])).toBeLessThan(30);
    expect(r.racers[0].passed).toBe(3);
  });
});

describe('kart race: ranking and items', () => {
  it('ranks by gates then distance; finish time beats progress', () => {
    const { r } = race();
    Object.assign(r.racers[0], { passed: 5, distNext: 100 });
    Object.assign(r.racers[1], { passed: 5, distNext: 40 });
    Object.assign(r.racers[2], { passed: 7, distNext: 300 });
    Object.assign(r.racers[3], { passed: 2 });
    expect(r.order()).toEqual([2, 1, 0, 3]);
    expect(r.rank(0)).toBe(2);
    r.racers[3].finish = 900; r.racers[0].finish = 800;
    expect(r.order()).toEqual([0, 3, 2, 1]);
  });
  it('roulette pays out after the spin; held items block pickups; rank skews the roll', () => {
    const { r, k } = race();
    r.phase = 'racing';
    const g = flat();
    const b = r.boxes[0].pos;
    k.pos = [b[0], 0, b[1]];
    r.tick(PAD, false, k, g);
    expect(r.racers[0].roulette).toBeGreaterThan(0);
    expect(r.racers[0].item).toBeNull();
    expect(r.drainEvents()).toContainEqual({ kind: 'pickup', i: 0 });
    expect(r.boxes[0].respawn).toBeGreaterThan(0);
    for (let i = 0; i < ROULETTE_TICKS; i++) r.tick(PAD, false, k, g);
    expect(r.racers[0].item).not.toBeNull();
    const held = r.racers[0].item;
    const b2 = r.boxes[1].pos;
    k.pos = [b2[0], 0, b2[1]];
    r.tick(PAD, false, k, g);
    expect(r.racers[0].item).toBe(held);
    expect(r.racers[0].roulette).toBe(0);
    const fresh = new KartRace(oval(), 1);
    fresh.rng = new Rng(99);
    let first = 0, last = 0;
    for (let i = 0; i < 400; i++) {
      if (fresh.roll(0) === 'mushroom') first++;
      if (fresh.roll(3) === 'mushroom') last++;
    }
    expect(last).toBeGreaterThan(first * 2);
  });
  it('mushroom boosts; a banana spins the kart and costs speed', () => {
    const { r, k } = race();
    r.phase = 'racing';
    const g = flat();
    for (let i = 0; i < 180; i++) r.tick(GO, false, k, g);
    const cruise = k.speed;
    r.racers[0].item = 'mushroom';
    let fastest = 0;
    r.tick(GO, true, k, g);
    for (let i = 0; i < BOOST_TICKS; i++) { r.tick(GO, false, k, g); fastest = Math.max(fastest, k.speed); }
    expect(fastest).toBeGreaterThan(cruise + 0.4);
    expect(r.racers[0].item).toBeNull();
    for (let i = 0; i < 80; i++) r.tick(GO, false, k, g);
    const before = k.speed;
    const f = fwd(k);
    r.bananas.push({ pos: [k.pos[0] + f[0] * 12, k.pos[2] + f[1] * 12], y: 0, owner: 1, age: 10 });
    r.racers[0].immune = 0;
    r.tick(GO, false, k, g);
    expect(r.racers[0].spin).toBeGreaterThan(0);
    expect(r.bananas).toHaveLength(0);
    expect(r.drainEvents()).toContainEqual({ kind: 'hit', i: 0 });
    for (let i = 0; i < 8; i++) r.tick(GO, false, k, g);
    expect(k.speed).toBeLessThan(before * 0.5);
    expect(r.spinAngle(0)).toBeGreaterThan(0);
    for (let i = 0; i < SPIN_TICKS; i++) r.tick(GO, false, k, g);
    expect(r.racers[0].spin).toBe(0);
    expect(r.racers[0].immune).toBeGreaterThan(0);
  });
  it('a dropped banana lands behind the kart and does not hit its owner at once', () => {
    const { r, k } = race();
    r.phase = 'racing';
    const g = flat();
    for (let i = 0; i < 90; i++) r.tick(GO, false, k, g);
    r.racers[0].item = 'banana';
    r.tick(GO, true, k, g);
    const b = r.bananas[0], f = fwd(k);
    expect((b.pos[0] - k.pos[0]) * f[0] + (b.pos[1] - k.pos[2]) * f[1]).toBeLessThan(0);
    expect(r.racers[0].spin).toBe(0);
  });
  it('a green shell flies straight, bounces off one wall, breaks on the second, expires and hits karts', () => {
    const r = new KartRace(oval(), 3);
    const g = flat(160);
    r.shells.push({ pos: [0, 0], y: 0, dir: [1, 0], bounces: 0, life: 0, owner: 0 });
    const seen: Shell[] = [];
    for (let i = 0; i < 40; i++) { r.stepProjectiles(g); if (r.shells[0]) seen.push({ ...r.shells[0] }); }
    expect(seen[0].pos[0]).toBeGreaterThan(25);
    expect(seen[0].pos[0]).toBeLessThan(35);
    expect(seen.some(s => s.bounces === 1 && s.dir[0] < 0)).toBe(true);
    for (let i = 0; i < SHELL_LIFE; i++) r.stepProjectiles(g);
    expect(r.shells).toHaveLength(0);
    r.shells.push({ pos: [150, 0], y: 0, dir: [1, 0], bounces: 1, life: 0, owner: 0 });
    for (let i = 0; i < 4; i++) r.stepProjectiles(g);
    expect(r.shells).toHaveLength(0);
    const { r: race2, k } = race();
    race2.phase = 'racing';
    const f = fwd(k);
    race2.shells.push({ pos: [k.pos[0] - f[0] * 60, k.pos[2] - f[1] * 60], y: 0, dir: f, bounces: 0, life: 20, owner: 1 });
    race2.racers[0].immune = 0;
    for (let i = 0; i < 4; i++) race2.tick(PAD, false, k, flat());
    expect(race2.racers[0].spin).toBeGreaterThan(0);
    expect(race2.shells).toHaveLength(0);
  });
  it('the xorshift stream matches the native twin', () => {
    const rng = new Rng(12345);
    expect([rng.nextU32(), rng.nextU32(), rng.nextU32()]).toEqual([3_336_926_330, 1_697_253_807, 2_816_511_904]);
  });
  it('formats lap times', () => {
    expect(formatTicks(0)).toBe('0:00.00');
    expect(formatTicks(30 * 75 + 15)).toBe('1:15.50');
  });
});

describe('kart race: CPU drivers', () => {
  it('lap a synthetic oval in order without rescues', () => {
    const { r, k } = race();
    const g = flat();
    for (let i = 0; i < 30 * 120; i++) {
      r.tick(PAD, false, k, g);
      if (r.racers.slice(1).every(x => x.laps.length >= 1)) break;
    }
    for (let c = 1; c < RACERS; c++) {
      expect(r.racers[c].laps.length).toBeGreaterThanOrEqual(1);
      expect(r.racers[c].rescues).toBe(0);
    }
  });
});

const TRACK = 'public/mk64/luigi-raceway.json';
describe.skipIf(!existsSync(TRACK))('kart race: real Luigi Raceway geometry', () => {
  interface Data { vertices: number[][]; groups: Record<string, number[]>; path: number[][]; scale: number }
  const data = JSON.parse(existsSync(TRACK) ? readFileSync(TRACK, 'utf8') : '{}') as Data;
  const UNIT = 8;
  function world(): CollisionWorld {
    const w = new CollisionWorld(1024, 100000);
    for (const [name, indices] of Object.entries(data.groups)) {
      const type = name.includes('Grass') ? SURFACE_NOT_SLIPPERY : 0;
      const p = (i: number): [number, number, number] => [data.vertices[i][0] * data.scale, data.vertices[i][1] * data.scale, data.vertices[i][2] * data.scale];
      for (let t = 0; t < indices.length; t += 3) w.addTriangle(p(indices[t]), p(indices[t + 1]), p(indices[t + 2]), { type });
    }
    w.finalize();
    return w;
  }
  const ground = (w: CollisionWorld): KartGround => ({
    floor: (x, y, z) => {
      const hit = w.findFloor(x * UNIT, y * UNIT, z * UNIT);
      if (!hit.surface) return null;
      return { height: hit.height / UNIT, normal: [hit.surface.normal[0], hit.surface.normal[1], hit.surface.normal[2]], grass: hit.surface.type === SURFACE_NOT_SLIPPERY };
    },
    wall: pos => {
      const data: WallCollisionData = { x: pos[0] * UNIT, y: pos[1] * UNIT, z: pos[2] * UNIT, offsetY: 50, radius: 65, numWalls: 0, walls: [] };
      if (!w.findWallCollisions(data)) return null;
      return {
        pos: [data.x / UNIT, data.y / UNIT, data.z / UNIT],
        hits: data.walls.map(s => ({ normal: [s.normal[0], s.normal[1], s.normal[2]], xFacing: (s.flags & SURFACE_FLAG_X_PROJECTION) !== 0 })),
      };
    },
  });

  it('three CPUs finish a full lap on the road, in checkpoint order, with the countdown', () => {
    const g = ground(world());
    const r = new KartRace(data.path, 20240607);
    const k = r.start(g);
    let ticks = 0;
    let grassTicks = 0;
    const seenPassed = [0, 0, 0, 0];
    for (; ticks < 30 * 90; ticks++) {
      r.tick(PAD, false, k, g);
      for (let c = 0; c < 3; c++) {
        const cpu = r.cpus[c];
        if (g.floor(cpu.kart.pos[0], cpu.kart.pos[1] + 5, cpu.kart.pos[2])?.grass) grassTicks++;
        expect(r.racers[c + 1].passed).toBeGreaterThanOrEqual(seenPassed[c + 1]);
        seenPassed[c + 1] = r.racers[c + 1].passed;
      }
      if (r.racers.slice(1).every(x => x.laps.length >= 1)) break;
    }
    for (let c = 1; c < RACERS; c++) {
      expect(r.racers[c].laps.length, `cpu ${c} at gate ${r.racers[c].passed}`).toBeGreaterThanOrEqual(1);
      expect(r.racers[c].rescues).toBe(0);
      expect(r.racers[c].laps[0]).toBeLessThan(30 * 70);
      expect(r.racers[c].laps[0]).toBeGreaterThan(30 * 30);
    }
    expect(grassTicks).toBeLessThan(ticks * 3 * 0.1);
  });
  it('the player can drive the grid out and the field is reproducible per seed', () => {
    const g = ground(world());
    const runOnce = () => {
      const r = new KartRace(data.path, 5);
      const k = r.start(g);
      for (let i = 0; i < 400; i++) r.tick(GO, false, k, g);
      return r.cpus.map(c => c.kart.pos.map(v => v.toFixed(3)).join(',')).join('|');
    };
    expect(runOnce()).toBe(runOnce());
  });
});
