import { describe, expect, it } from 'vitest';
import { KartPhysics, TOP_SPEED, type KartGround, type KartInput } from './kartPhysics';

const flat = (grass = false, wallX?: number): KartGround => ({
  floor: () => ({ height: 0, normal: [0, 1, 0], grass }),
  wall: pos => (wallX !== undefined && pos[0] > wallX ? [wallX, pos[1], pos[2]] : null),
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
  it('matches the native trace', () => {
    const g = flat();
    const k = new KartPhysics([0, 0, 0], 0);
    const out: string[] = [];
    for (let f = 0; f < 600; f++) {
      k.step({
        ...GAS,
        rawX: f >= 200 && f < 260 ? 80 : f >= 300 && f < 330 ? -60 : 0,
        hopPressed: f === 420,
        hopDown: f >= 420 && f < 470,
      }, g);
      if (f % 100 === 99) out.push(`${f} ${k.speed.toFixed(4)} ${k.pos[0].toFixed(3)} ${k.pos[2].toFixed(3)} ${k.yaw().toFixed(3)}`);
    }
    expect(out.join('\n').replace(/-0\.000/g, '0.000')).toBe([
      '99 1.7338 0.000 56.379 0.000',
      '199 3.8221 0.000 332.509 0.000',
      '299 4.6568 -240.636 597.347 -1.317',
      '399 5.6585 -630.322 909.593 -0.703',
      '499 5.8828 -1005.732 1352.218 -0.703',
      '599 5.8850 -1386.374 1801.003 -0.703',
    ].join('\n'));
  });
});
