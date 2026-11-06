import { describe, expect, it } from 'vitest';
import { RailPath, unfold } from '../src/game/experiments/railPath';

describe('2.5D rail', () => {
  it('drops dead-end spikes that fold the rail back on itself', () => {
    const route: Array<[number, number, number]> = [[0, 0, 0], [1000, 0, 0], [1600, 0, 0], [1200, 0, 40], [2400, 0, 600]];
    const cleaned = unfold(route);
    for (let i = 1; i < cleaned.length - 1; i++) {
      const [a, b, c] = [cleaned[i - 1], cleaned[i], cleaned[i + 1]];
      const dot = (b[0] - a[0]) * (c[0] - b[0]) + (b[2] - a[2]) * (c[2] - b[2]);
      expect(dot).toBeGreaterThan(0);
    }
  });

  it('projects onto the nearest point and reports a forward tangent', () => {
    const rail = new RailPath([[0, 0, 0], [2000, 0, 0], [2000, 0, 2000]]);
    const hit = rail.nearest([300, 0, 400]);
    // The corner is rounded, but the projection is still the closest rail point.
    for (let s = 0; s <= rail.total; s += 50) {
      const p = rail.at(s).point;
      expect(Math.hypot(p[0] - 300, p[2] - 400)).toBeGreaterThanOrEqual(Math.hypot(hit.point[0] - 300, hit.point[2] - 400) - 1);
    }
    expect(hit.tangent[0]).toBeGreaterThan(0.9);
    expect(rail.at(rail.total).point[2]).toBeCloseTo(2000, 0);
  });
});
