import type { Vec3 } from '../../sim/math';

export interface RailHit {
  /** Distance along the rail. */
  s: number;
  /** Closest rail point (horizontal); y is the rail's authored height there. */
  point: Vec3;
  /** Horizontal unit tangent toward increasing s. */
  tangent: [number, number];
  index: number;
}

/**
 * A horizontal rail for 2.5D play: Mario's x/z are projected onto it every
 * step while his height stays free (SM64 physics handle slopes and jumps).
 * Corners from the route planner are rounded with a Catmull-Rom spline.
 */
export class RailPath {
  readonly points: Vec3[];
  readonly lengths: number[];
  readonly total: number;

  constructor(route: readonly Vec3[], spacing = 60) {
    this.points = resample(smooth(route), spacing);
    this.lengths = [0];
    for (let i = 1; i < this.points.length; i++) {
      const a = this.points[i - 1], b = this.points[i];
      this.lengths.push(this.lengths[i - 1] + Math.hypot(b[0] - a[0], b[2] - a[2]));
    }
    this.total = this.lengths[this.lengths.length - 1];
  }

  /** Nearest point to `pos`, searching around `hint` (the last index) when given. */
  nearest(pos: Vec3, hint = -1, window = 40): RailHit {
    const from = hint < 0 ? 0 : Math.max(0, hint - window);
    const to = hint < 0 ? this.points.length - 2 : Math.min(this.points.length - 2, hint + window);
    let best: RailHit | null = null;
    let bestD = Infinity;
    for (let i = from; i <= to; i++) {
      const a = this.points[i], b = this.points[i + 1];
      const dx = b[0] - a[0], dz = b[2] - a[2];
      const len2 = dx * dx + dz * dz || 1;
      const t = Math.max(0, Math.min(1, ((pos[0] - a[0]) * dx + (pos[2] - a[2]) * dz) / len2));
      const x = a[0] + dx * t, z = a[2] + dz * t;
      // Prefer the layer at Mario's height where the rail passes over itself.
      const y = a[1] + (b[1] - a[1]) * t;
      const d = Math.hypot(pos[0] - x, pos[2] - z) + Math.max(0, Math.abs(pos[1] - y) - 400) * 2;
      if (d < bestD) {
        bestD = d;
        const len = Math.sqrt(len2);
        best = { s: this.lengths[i] + len * t, point: [x, y, z], tangent: [dx / len, dz / len], index: i };
      }
    }
    return best!;
  }

  /** Point and tangent at distance s (clamped). */
  at(s: number): RailHit {
    const clamped = Math.max(0, Math.min(this.total, s));
    let lo = 0, hi = this.lengths.length - 1;
    while (hi - lo > 1) {
      const mid = (lo + hi) >> 1;
      if (this.lengths[mid] <= clamped) lo = mid; else hi = mid;
    }
    const a = this.points[lo], b = this.points[hi];
    const seg = this.lengths[hi] - this.lengths[lo] || 1;
    const t = (clamped - this.lengths[lo]) / seg;
    const dx = b[0] - a[0], dz = b[2] - a[2];
    const len = Math.hypot(dx, dz) || 1;
    return { s: clamped, point: [a[0] + dx * t, a[1] + (b[1] - a[1]) * t, a[2] + dz * t], tangent: [dx / len, dz / len], index: lo };
  }
}

/** Centripetal-ish Catmull-Rom through the route, 6 samples per segment. */
function smooth(route: readonly Vec3[]): Vec3[] {
  if (route.length < 3) return route.map(p => [...p] as Vec3);
  const out: Vec3[] = [];
  for (let i = 0; i < route.length - 1; i++) {
    const p0 = route[Math.max(0, i - 1)], p1 = route[i], p2 = route[i + 1], p3 = route[Math.min(route.length - 1, i + 2)];
    for (let k = 0; k < 6; k++) {
      const t = k / 6, t2 = t * t, t3 = t2 * t;
      const c = (a: number, b: number, c2: number, d: number) => 0.5 * (2 * b + (-a + c2) * t + (2 * a - 5 * b + 4 * c2 - d) * t2 + (-a + 3 * b - 3 * c2 + d) * t3);
      out.push([c(p0[0], p1[0], p2[0], p3[0]), c(p0[1], p1[1], p2[1], p3[1]), c(p0[2], p1[2], p2[2], p3[2])]);
    }
  }
  out.push([...route[route.length - 1]] as Vec3);
  return out;
}

function resample(points: readonly Vec3[], spacing: number): Vec3[] {
  const out: Vec3[] = [[...points[0]] as Vec3];
  let carry = 0;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1], b = points[i];
    const len = Math.hypot(b[0] - a[0], b[2] - a[2]);
    let d = spacing - carry;
    while (d <= len) {
      const t = d / len;
      out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
      d += spacing;
    }
    carry = len - (d - spacing);
  }
  const last = points[points.length - 1];
  if (Math.hypot(last[0] - out[out.length - 1][0], last[2] - out[out.length - 1][2]) > 1) out.push([...last] as Vec3);
  return out;
}
