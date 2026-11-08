import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { KartPhysics, TOP_SPEED, type KartGround, type KartInput } from './kartPhysics';

const flat = (grass = false, wallX?: number): KartGround => ({
  floor: () => ({ height: 0, normal: [0, 1, 0], grass }),
  wall: pos => (wallX !== undefined && pos[0] > wallX ? { pos: [wallX, pos[1], pos[2]], hits: [{ normal: [-1, 0, 0], xFacing: true }] } : null),
});
const GAS: KartInput = { rawX: 0, rawY: 80, accel: true, brake: false, hopDown: false, hopPressed: false };
const run = (k: KartPhysics, g: KartGround, n: number, i: KartInput) => { for (let f = 0; f < n; f++) k.step(i, g); };

describe('MK64 kart physics port', () => {
  it('reaches the decomp top speed in about five seconds', () => {
    const k = new KartPhysics([0, 0, 0], 0);
    run(k, flat(), 180, GAS);
    const at3 = k.speed;
    run(k, flat(), 360, GAS);
    expect(k.currentSpeed).toBe(TOP_SPEED);
    expect(k.speed).toBeCloseTo(4096 / 696, 1);
    expect(at3).toBeGreaterThan(2.5);
    expect(at3).toBeLessThan(k.speed - 0.3);
  });
  it('grass costs most of the speed, walls stop the kart', () => {
    const k = new KartPhysics([0, 0, 0], 0);
    run(k, flat(true), 600, GAS);
    expect(k.speed).toBeGreaterThan(1.5);
    expect(k.speed).toBeLessThan(2.6);
    const w = new KartPhysics([0, 0, 0], -16384);
    run(w, flat(false, 200), 900, GAS);
    expect(w.pos[0]).toBeLessThanOrEqual(200.5);
  });
  it('hops, drifts and earns a mini-turbo by counter-steering', () => {
    const g = flat();
    const k = new KartPhysics([0, 0, 0], 0);
    run(k, g, 600, GAS);
    k.step({ ...GAS, rawX: 70, hopPressed: true, hopDown: true }, g);
    expect(k.hop && k.midair && k.pos[1] > 0.5 && k.drifting).toBe(true);
    run(k, g, 20, { ...GAS, rawX: 70, hopDown: true });
    expect(k.drifting).toBe(true);
    for (let i = 0; i < 4 && k.driftState < 2; i++) {
      run(k, g, 24, { ...GAS, rawX: -70, hopDown: true });
      run(k, g, 6, { ...GAS, rawX: 70, hopDown: true });
    }
    expect(k.driftState).toBeGreaterThanOrEqual(2);
    run(k, g, 2, GAS);
    expect(k.miniTurbo).toBe(true);
  });
  it('slopes pitch the kart, crests launch it and walls bounce it (decomp ground contact)', () => {
    const k = new KartPhysics([0, 0, 0], 0);
    let minSlope = 0, maxSlope = 0, air = 0, high = 0;
    for (let f = 0; f < 500; f++) {
      k.step(GAS, hills);
      minSlope = Math.min(minSlope, Math.trunc(k.slopeAccel / 182));
      maxSlope = Math.max(maxSlope, Math.trunc(k.slopeAccel / 182));
      air += k.midair ? 1 : 0;
      high = Math.max(high, k.pos[1] - hillHeight(k.pos[2]));
    }
    expect(minSlope).toBeLessThanOrEqual(-4);
    expect(maxSlope).toBeGreaterThanOrEqual(3);
    expect(air).toBeGreaterThan(3);
    expect(high).toBeGreaterThan(1);
    const w = new KartPhysics([198, 0, 0], -16384);
    w.velocity = [5, 0, 0]; w.currentSpeed = TOP_SPEED; w.speed = 5;
    w.step(GAS, flat(false, 200));
    expect(w.bumped).toBe(true);
    expect(w.velocity[0]).toBeLessThan(0);
    expect(Math.abs(w.velocity[0])).toBeLessThan(5);
    expect(w.currentSpeed).toBeLessThan(TOP_SPEED - 10);
    expect(w.drifting).toBe(false);
  });
  it('a braked kart holds on a hill and sliding along a wall does not stick', () => {
    const k = new KartPhysics([0, 0, 600], 0);
    run(k, hills, 90, GAS);
    run(k, hills, 400, { ...GAS, rawX: 0, rawY: 0, accel: false, brake: true });
    expect(k.speed).toBeLessThan(0.5);
    const s = new KartPhysics([195, 0, 0], 0);
    run(s, flat(false, 200), 200, GAS);
    expect(s.pos[2]).toBeGreaterThan(300);
  });
  it('matches the native trace', () => {
    const golden = readFileSync(new URL('../../../native/src/experiments/kart_physics_trace.txt', import.meta.url), 'utf8');
    expect(trace().join('\n').replace(/-0\.000/g, '0.000').trim()).toBe(golden.trim());
  });
});

/** Rolling course shared with kart_physics.rs: flat, a 15% climb z=300..700, a 30% descent after the crest,
 * a wall at x=-150 (faces +x) and one at z=1500 (faces -z). */
const hillSlope = (z: number) => (z < 300 ? 0 : z < 700 ? 0.15 : -0.3);
const hillHeight = (z: number) => (z < 300 ? 0 : z < 700 ? 0.15 * (z - 300) : 60 - 0.3 * (z - 700));
const hills: KartGround = {
  floor: (_x, _y, z) => {
    const g = hillSlope(z), len = Math.sqrt(1 + g * g);
    return { height: hillHeight(z), normal: [0, 1 / len, -g / len], grass: false };
  },
  wall: pos => {
    const out = { pos: [...pos] as [number, number, number], hits: [] as { normal: [number, number, number]; xFacing: boolean }[] };
    if (pos[0] < -150) { out.pos[0] = -150; out.hits.push({ normal: [1, 0, 0], xFacing: true }); }
    if (pos[2] > 1500) { out.pos[2] = 1500; out.hits.push({ normal: [0, 0, -1], xFacing: false }); }
    return out.hits.length ? out : null;
  },
};

function trace(): string[] {
  const out: string[] = [];
  const g = flat();
  let k = new KartPhysics([0, 0, 0], 0);
  for (let f = 0; f < 600; f++) {
    k.step({
      ...GAS,
      rawX: f >= 200 && f < 260 ? 80 : f >= 300 && f < 330 ? -60 : 0,
      hopPressed: f === 420,
      hopDown: f >= 420 && f < 470,
    }, g);
    if (f % 100 === 99) out.push(`${f} ${k.speed.toFixed(4)} ${k.pos[0].toFixed(3)} ${k.pos[2].toFixed(3)} ${k.yaw().toFixed(3)}`);
  }
  // Hills: climb, crest and land, bounce off both walls, reverse, brake on the slope.
  k = new KartPhysics([0, 0, 0], 0);
  let bumps = 0, air = 0;
  for (let f = 0; f < 900; f++) {
    const i: KartInput = f < 400 ? GAS
      : f < 440 ? { ...GAS, rawX: 80 }
      : f < 600 ? GAS
      : f < 660 ? { ...GAS, rawX: 80, hopPressed: f === 600, hopDown: true }
      : f < 750 ? { ...GAS, rawX: 0, rawY: -80, accel: false, brake: true }
      : { ...GAS, rawX: 0, rawY: 0, accel: false, brake: true };
    k.step(i, hills);
    bumps += k.bumped ? 1 : 0;
    air += k.midair ? 1 : 0;
    if (f % 100 === 99) out.push(`H${f} ${k.speed.toFixed(4)} ${k.pos[0].toFixed(3)} ${k.pos[1].toFixed(3)} ${k.pos[2].toFixed(3)} ${k.yaw().toFixed(3)} ${Math.trunc(k.slopeAccel / 182)} ${bumps} ${air}`);
  }
  return out;
}
