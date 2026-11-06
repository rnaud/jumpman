import type { CollisionWorld, Surface } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { SURFACE_DEATH_PLANE } from '../../sim/surfaces';

const CELL = 150;
const STEP_UP = 95;
const JUMP_UP = 210;
const DROP = 700;

/** Minimum floor normal Mario can run up, by surface type (mario_floor_is_slippery thresholds plus margin). */
export function walkableNormal(surface: Surface): number {
  switch (surface.type) {
    case 0x13: return 0.9848; // very slippery: 10°
    case 0x14: return 0.9397; // slippery: 20°
    case 0x15: return 0; // not slippery
    default: return 0.788; // 38°
  }
}

function standable(world: CollisionWorld, x: number, z: number, probe: number): number | null {
  const floor = world.findFloor(x, probe, z);
  const s = floor.surface;
  if (!s || s.type === SURFACE_DEATH_PLANE || s.normal[1] < walkableNormal(s)) return null;
  if (world.waterLevel(x, z) > floor.height - 30) return null;
  return floor.height;
}

/** Whether Mario can run the straight segment a→b without sliding or climbing walls. */
export function segmentWalkable(world: CollisionWorld, a: Vec3, b: Vec3): boolean {
  const length = Math.hypot(b[0] - a[0], b[2] - a[2]);
  const steps = Math.max(1, Math.ceil(length / 100));
  let previous = standable(world, a[0], a[2], a[1] + 300);
  if (previous === null) return false;
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const h = standable(world, a[0] + (b[0] - a[0]) * t, a[2] + (b[2] - a[2]) * t, Math.max(previous, a[1] + (b[1] - a[1]) * t) + 300);
    if (h === null || h - previous > STEP_UP || previous - h > DROP) return false;
    previous = h;
  }
  return true;
}

/** Grid A* over the course's top floors between two points; null when no path is found. */
export function findWalkPath(world: CollisionWorld, a: Vec3, b: Vec3, margin = 2600, budget = 40000, stats?: { expansions: number; startOk: boolean }): Vec3[] | null {
  const minX = Math.min(a[0], b[0]) - margin, minZ = Math.min(a[2], b[2]) - margin;
  const cols = Math.ceil((Math.abs(a[0] - b[0]) + margin * 2) / CELL);
  const rows = Math.ceil((Math.abs(a[2] - b[2]) + margin * 2) / CELL);
  // Probe just above the height the segment would have here, so stacked
  // terrain (spiral paths, bridges) resolves to the layer being walked.
  const sx = b[0] - a[0], sz = b[2] - a[2];
  const length2 = Math.max(1, sx * sx + sz * sz);
  const probeAt = (x: number, z: number) => {
    const t = Math.max(0, Math.min(1, ((x - a[0]) * sx + (z - a[2]) * sz) / length2));
    return a[1] + (b[1] - a[1]) * t + 700;
  };
  const heights = new Float32Array(cols * rows).fill(NaN);
  const known = new Uint8Array(cols * rows);
  const height = (i: number) => {
    if (!known[i]) {
      known[i] = 1;
      const x = minX + (i % cols + 0.5) * CELL, z = minZ + (Math.floor(i / cols) + 0.5) * CELL;
      const h = standable(world, x, z, probeAt(x, z));
      heights[i] = h === null ? NaN : h;
    }
    return heights[i];
  };
  const cellOf = (p: Vec3) => Math.floor((p[2] - minZ) / CELL) * cols + Math.floor((p[0] - minX) / CELL);
  const start = cellOf(a), goal = cellOf(b);
  const gx = goal % cols, gz = Math.floor(goal / cols);
  const g = new Float32Array(cols * rows).fill(Infinity);
  const came = new Int32Array(cols * rows).fill(-1);
  const closed = new Uint8Array(cols * rows);
  // Binary heap of [f, index].
  const heap: Array<[number, number]> = [];
  const push = (f: number, i: number) => {
    heap.push([f, i]);
    let k = heap.length - 1;
    while (k > 0) {
      const parent = (k - 1) >> 1;
      if (heap[parent][0] <= heap[k][0]) break;
      [heap[parent], heap[k]] = [heap[k], heap[parent]];
      k = parent;
    }
  };
  const pop = () => {
    const top = heap[0];
    const last = heap.pop()!;
    if (heap.length) {
      heap[0] = last;
      let k = 0;
      for (;;) {
        const l = k * 2 + 1, r = l + 1;
        let m = k;
        if (l < heap.length && heap[l][0] < heap[m][0]) m = l;
        if (r < heap.length && heap[r][0] < heap[m][0]) m = r;
        if (m === k) break;
        [heap[m], heap[k]] = [heap[k], heap[m]];
        k = m;
      }
    }
    return top;
  };
  if (stats) stats.startOk = !Number.isNaN(height(start));
  if (Number.isNaN(height(start))) return null;
  g[start] = 0;
  push(0, start);
  let expansions = 0;
  let found = -1;
  while (heap.length && expansions++ < budget) {
    const [, current] = pop();
    if (closed[current]) continue;
    closed[current] = 1;
    const cx = current % cols, cz = Math.floor(current / cols);
    const h0 = height(current);
    if (Math.abs(cx - gx) <= 1 && Math.abs(cz - gz) <= 1 && Math.abs(h0 - b[1]) < 250) { found = current; break; }
    for (let dz = -1; dz <= 1; dz++) for (let dx = -1; dx <= 1; dx++) {
      if (!dx && !dz) continue;
      const nx = cx + dx, nz = cz + dz;
      if (nx < 0 || nz < 0 || nx >= cols || nz >= rows) continue;
      const next = nz * cols + nx;
      if (closed[next]) continue;
      const h1 = height(next);
      if (Number.isNaN(h1)) continue;
      const rise = h1 - h0;
      if (rise > JUMP_UP || -rise > DROP) continue;
      const step = (dx && dz ? 1.414 : 1) * CELL + Math.max(0, rise) * 1.5 + (rise > STEP_UP ? 600 : 0);
      const cost = g[current] + step;
      if (cost >= g[next]) continue;
      g[next] = cost;
      came[next] = current;
      push(cost + Math.hypot(nx - gx, nz - gz) * CELL, next);
    }
  }
  if (stats) stats.expansions = expansions;
  if (found < 0) return null;
  const cells: number[] = [];
  for (let i = found; i >= 0; i = came[i]) cells.push(i);
  cells.reverse();
  const points: Vec3[] = cells.map(i => [minX + (i % cols + 0.5) * CELL, heights[i], minZ + (Math.floor(i / cols) + 0.5) * CELL]);
  // Keep only the corners: drop points whose shortcut is still walkable.
  const simplified: Vec3[] = [points[0]];
  let anchor = 0;
  for (let i = 2; i < points.length; i++) {
    if (!segmentWalkable(world, points[anchor], points[i]) || Math.hypot(points[i][0] - points[anchor][0], points[i][2] - points[anchor][2]) > 1400) {
      simplified.push(points[i - 1]);
      anchor = i - 1;
    }
  }
  simplified.push(b);
  return simplified;
}

/**
 * Turn a route authored for another walker (e.g. Koopa the Quick, who ignores
 * slippery slopes) into one Mario can actually run, detouring where needed.
 */
export function marioRoute(world: CollisionWorld, route: readonly Vec3[]): Vec3[] {
  // Waypoints sitting on slopes Mario slides down move to the nearest standable spot.
  const points = route.map(point => nearestStandable(world, point) ?? point);
  const out: Vec3[] = [points[0]];
  for (let i = 1; i < points.length; i++) {
    const b = points[i];
    // Reach b from the latest route point that can; backtracking skips dead ends
    // (e.g. a pit whose only exit is a slope Mario slides down).
    let joined = false;
    for (let j = out.length - 1; j >= Math.max(0, out.length - 24) && !joined; j--) {
      const a = out[j];
      if (segmentWalkable(world, a, b)) {
        out.length = j + 1;
        out.push(b);
        joined = true;
        break;
      }
      const detour = findWalkPath(world, a, b, 3200, 150000);
      if (detour) {
        out.length = j + 1;
        out.push(...detour.slice(1));
        joined = true;
      }
    }
    // No walkable plan: keep the checkpoint; racers jump and, if needed, warp past it.
    if (!joined) out.push(b);
  }
  return out;
}

/** The closest point near `p` where Mario can stand, preferring similar height. */
export function nearestStandable(world: CollisionWorld, p: Vec3, radius = 1500): Vec3 | null {
  const probe = p[1] + 400;
  const here = standable(world, p[0], p[2], probe);
  if (here !== null) return [p[0], here, p[2]];
  let best: Vec3 | null = null;
  let score = Infinity;
  for (let r = 100; r <= radius; r += 100) {
    const samples = Math.ceil((Math.PI * 2 * r) / 100);
    for (let i = 0; i < samples; i++) {
      const a = (i / samples) * Math.PI * 2;
      const x = p[0] + Math.cos(a) * r, z = p[2] + Math.sin(a) * r;
      const h = standable(world, x, z, probe);
      if (h === null) continue;
      const s = r + Math.abs(h - p[1]) * 1.5;
      if (s < score) { score = s; best = [x, h, z]; }
    }
    if (best && score < r + 200) break;
  }
  return best;
}
