import { describe, expect, it } from 'vitest';
import { atan2s, coss, sins, toS16 } from '../src/sim/math';

describe('SM64 angle maths', () => {
  it('maps cardinal directions the way SM64 yaw does', () => {
    // yaw 0 = +Z, 0x4000 = +X
    expect(atan2s(1, 0)).toBe(0);
    expect(atan2s(0, 1)).toBe(0x4000);
    expect(atan2s(-1, 0)).toBe(-0x8000);
    expect(atan2s(0, -1)).toBe(-0x4000);
  });

  it('has sine on X and cosine on Z', () => {
    expect(sins(0)).toBeCloseTo(0, 6);
    expect(coss(0)).toBeCloseTo(1, 6);
    expect(sins(0x4000)).toBeCloseTo(1, 6);
    expect(coss(0x4000)).toBeCloseTo(0, 6);
  });

  it('wraps angles to signed 16-bit', () => {
    expect(toS16(0x8000)).toBe(-0x8000);
    expect(toS16(0x10000)).toBe(0);
    expect(toS16(-0x4001)).toBe(-0x4001);
    expect(toS16(0x17fff)).toBe(0x7fff);
  });

  it('is consistent with the atan2s used for floor angles', () => {
    // A floor tilted down toward +Z has normal (0, cos, sin-ish): yaw 0.
    const angle = atan2s(0.5, 0.5);
    expect(angle).toBeCloseTo(0x2000, -1);
  });
});
