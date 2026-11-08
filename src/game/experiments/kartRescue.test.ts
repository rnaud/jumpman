import { describe, expect, it } from 'vitest';
import { FADE_IN, FADE_OUT, FALL_MARGIN, HOLD, Lakitu } from './kartRescue';

type Vec3 = [number, number, number];
// 40 nodes round a circle of radius 4000 (world units), plus the export's sentinel.
const ring = (): Vec3[] => [
  ...Array.from({ length: 40 }, (_, i): Vec3 => { const a = i / 40 * Math.PI * 2; return [Math.sin(a) * 4000, -400, Math.cos(a) * 4000]; }),
  [-32768 * 8, -32768 * 8, -32768 * 8],
];

describe('Lakitu rescue', () => {
  it('picks the nearest node to the last grounded spot and faces down the track', () => {
    const l = new Lakitu(ring());
    l.noteGrounded([4000, -400, 20]);
    const d = l.destination([0, -9000, 0])!;
    expect(Math.abs(d.pos[0] - 4000)).toBeLessThan(200);
    expect(Math.abs(d.pos[2])).toBeLessThan(600);
    // Node 10 sits at +x; the next nodes run towards -z, so the kart faces -z.
    expect(Math.cos(d.yaw)).toBeLessThan(-0.5);
  });
  it('only triggers well below the course and ignores the sentinel', () => {
    const l = new Lakitu(ring());
    expect(l.shouldRescue([0, -500, 0])).toBe(false);
    expect(l.shouldRescue([0, -1000, 0])).toBe(false);
    expect(l.shouldRescue([0, -400 - FALL_MARGIN - 1, 0])).toBe(true);
    expect(new Lakitu([]).shouldRescue([0, -1e9, 0])).toBe(false);
  });
  it('fades out, teleports once and fades back in', () => {
    const l = new Lakitu(ring());
    l.start();
    let teleports = 0, ticks = 0, peak = 0;
    while (l.active) {
      if (l.tick()) { teleports++; expect(l.alpha).toBeCloseTo(1); }
      ticks++;
      peak = Math.max(peak, l.alpha);
    }
    expect(teleports).toBe(1);
    expect(ticks).toBe(FADE_OUT + HOLD + FADE_IN);
    expect(peak).toBe(1);
    expect(l.alpha).toBe(0);
    expect(l.holding).toBe(false);
  });
});
