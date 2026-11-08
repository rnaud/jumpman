import { describe, expect, it } from 'vitest';
import { CAMERA_HEIGHT, KartCamera, ORIGIN_LIFT, adjustAngle, lerpPose, type CameraKart } from './kartCamera';

const DEG = 182;
const s16 = (v: number) => (Math.trunc(v) << 16) >> 16;
const kart = (pos: [number, number, number], rotation: number, extra: Partial<CameraKart> = {}): CameraKart => ({
  pos, rotation, drifting: false, driftOutside: false, turn: 0, kmh: 100, hop: false, bumped: false, mushroom: false, spinout: false, ...extra,
});
const forward = (rotation: number): [number, number] => { const a = -rotation * Math.PI / 32768; return [Math.sin(a), Math.cos(a)]; };
const behind = (c: KartCamera, k: CameraKart) => {
  const f = forward(k.rotation);
  return -((c.eye[0] - k.pos[0]) * f[0] + (c.eye[2] - k.pos[2]) * f[1]);
};

describe('MK64 chase camera (camera.c)', () => {
  it('adjust_angle takes the short way and never overshoots', () => {
    expect(adjustAngle(0, 1000, 300)).toBe(300);
    expect(adjustAngle(0, 100, 300)).toBe(100);
    expect(adjustAngle(0, -1000, 300)).toBe(-300);
    expect(adjustAngle(32000, -32000, 500)).toBe(s16(32000 + 500));
    expect(adjustAngle(7, 7, 100)).toBe(7);
  });

  it('straight driving settles fifty behind and nine and a half up', () => {
    const c = new KartCamera();
    const k = kart([100, 0, 200], 0);
    c.snap(k);
    for (let i = 0; i < 120; i++) { k.pos[2] += 5; c.step(k); }
    expect(Math.abs(behind(c, k) - 50)).toBeLessThan(8);
    expect(c.eye[1]).toBeCloseTo(ORIGIN_LIFT + CAMERA_HEIGHT, 6);
    expect(c.look[1]).toBeCloseTo(ORIGIN_LIFT, 6);
    const ahead = Math.abs(c.look[2] - k.pos[2]);
    expect(ahead).toBeGreaterThan(60);
    expect(ahead).toBeLessThan(72);
    expect(c.fov).toBe(40);
  });

  it('a parked kart is exactly at the decomp offsets', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    for (let i = 0; i < 60; i++) c.step(k);
    expect(behind(c, k)).toBeCloseTo(50, 6);
    const f = forward(0);
    expect(c.look[0] * f[0] + c.look[2] * f[1]).toBeCloseTo(70, 6);
  });

  it('the heading lags a turn, catches up, and follows faster the harder the turn', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    k.rotation = 8000; k.turn = 30;
    c.step(k);
    expect(c.heading).toBeGreaterThan(0);
    expect(c.heading).toBeLessThan(8000);
    for (let i = 0; i < 120; i++) c.step(k);
    expect(c.heading).toBe(8000);
    const slow = new KartCamera(), hard = new KartCamera();
    const a = kart([0, 0, 0], 0);
    slow.snap(a); hard.snap(a);
    a.rotation = 20000; a.turn = 10; slow.step(a);
    a.turn = 120; hard.step(a);
    expect(hard.heading).toBeGreaterThan(slow.heading);
  });

  it('drifting swings the camera twelve degrees towards the inside and relaxes afterwards', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    k.drifting = true; k.turn = 60;
    for (let i = 0; i < 60; i++) c.step(k);
    expect(Math.abs(c.swing - 12 * DEG)).toBeLessThan(30);
    k.turn = -60;
    for (let i = 0; i < 60; i++) c.step(k);
    expect(Math.abs(c.swing + 12 * DEG)).toBeLessThan(30);
    k.drifting = false;
    for (let i = 0; i < 200; i++) c.step(k);
    expect(Math.abs(c.swing)).toBeLessThan(5);
  });

  it('a spinout freezes the heading and widens the view to 58 degrees', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    k.spinout = true; k.rotation = 12000; k.turn = 50;
    for (let i = 0; i < 30; i++) c.step(k);
    expect(c.heading).toBe(0);
    expect(c.fov).toBeCloseTo(58, 1);
    k.spinout = false;
    for (let i = 0; i < 40; i++) c.step(k);
    expect(c.fov).toBeCloseTo(40, 1);
  });

  it('a mushroom opens the view one degree a frame and pushes the camera in', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    k.mushroom = true;
    c.step(k);
    expect(c.fov).toBeCloseTo(41, 9);
    let closest = Infinity;
    for (let i = 0; i < 40; i++) { c.step(k); closest = Math.min(closest, behind(c, k)); }
    expect(c.fov).toBeCloseTo(48, 1);
    expect(closest).toBeLessThan(45);
  });

  it('a wall bump above 30 km/h kicks the camera up once; a slow one does nothing', () => {
    const c = new KartCamera();
    const k = kart([0, 0, 0], 0);
    c.snap(k);
    k.bumped = true; c.step(k); k.bumped = false;
    let peak = -Infinity;
    for (let i = 0; i < 12; i++) { c.step(k); peak = Math.max(peak, c.look[1]); }
    expect(peak).toBeGreaterThan(ORIGIN_LIFT + 0.5);
    for (let i = 0; i < 200; i++) c.step(k);
    expect(c.look[1]).toBeCloseTo(ORIGIN_LIFT, 3);
    const slow = new KartCamera();
    const s = kart([0, 0, 0], 0, { kmh: 10 });
    slow.snap(s);
    s.bumped = true; slow.step(s);
    expect(slow.look[1]).toBeCloseTo(ORIGIN_LIFT, 9);
  });

  it('blends poses between ticks', () => {
    const m = lerpPose({ eye: [0, 0, 0], look: [10, 0, 0], fov: 40 }, { eye: [2, 4, 6], look: [10, 0, 8], fov: 48 }, 0.5);
    expect(m.eye).toEqual([1, 2, 3]);
    expect(m.look).toEqual([10, 0, 4]);
    expect(m.fov).toBe(44);
  });
});
