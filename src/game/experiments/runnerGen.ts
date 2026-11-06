/**
 * Endless runner — the pure, deterministic track generator.
 *
 * The track runs along +X. It is a chain of segments; each starts where the
 * last ended (`end`), so the course is continuous by construction. Every gap
 * and step is clamped against Mario's measured running jump.
 */

/** Measured with MarioRuntime at full run speed (see tests/runnerGen.test.ts). */
export const RUNNING_JUMP = { distance: 841, height: 335 } as const;
export const LIMITS = {
  /** Longest gap, easy → hard (fraction of a running single jump). */
  gap: [260, 460] as const,
  /** Highest step up onto the next floor. */
  rise: [90, 160] as const,
  /** Hurdles must clear with a plain jump. */
  hurdle: 110,
};

export type BlockKind = 'floor' | 'hurdle' | 'pillar' | 'bridge';
export interface Block {
  kind: BlockKind;
  /** Start and end along X. */
  x0: number;
  x1: number;
  /** Centre and width across Z. */
  z: number;
  width: number;
  /** Top surface height and thickness below it. */
  top: number;
  height: number;
  /** Visual theme index. */
  theme: number;
}

export interface Segment {
  index: number;
  kind: string;
  difficulty: number;
  theme: number;
  blocks: Block[];
  coins: Array<[number, number, number]>;
  start: number;
  end: number;
  /** Floor height where the next segment must begin. */
  exitTop: number;
}

export const THEMES = [
  { name: 'Grass Run', from: 0 },
  { name: 'Brick Fortress', from: 9 },
  { name: 'Rainbow Sky', from: 18 },
] as const;

export function rng(seed: number): () => number {
  let state = (seed >>> 0) || 0x2545f491;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function difficultyAt(index: number): number {
  return Math.min(1, Math.max(0, (index - 2) / 30));
}

export function themeAt(index: number): number {
  let theme = 0;
  THEMES.forEach((entry, i) => { if (index >= entry.from) theme = i; });
  return theme;
}

const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const maxGap = (d: number) => lerp(LIMITS.gap[0], LIMITS.gap[1], d);
export const maxRise = (d: number) => lerp(LIMITS.rise[0], LIMITS.rise[1], d);

const WIDE = 1100;

export class RunnerTrack {
  private readonly segments: Segment[] = [];
  constructor(readonly seed: number, readonly originX: number, readonly originTop: number) {}

  segment(index: number): Segment {
    while (this.segments.length <= index) this.segments.push(this.build(this.segments.length));
    return this.segments[index];
  }

  private build(index: number): Segment {
    const random = rng(this.seed * 92821 + index * 68917);
    const previous = index > 0 ? this.segments[index - 1] : null;
    const start = previous ? previous.end : this.originX;
    const top = previous ? previous.exitTop : this.originTop;
    const difficulty = difficultyAt(index);
    const theme = themeAt(index);
    const seg: Segment = { index, kind: 'run', difficulty, theme, blocks: [], coins: [], start, end: start, exitTop: top };
    const range = (a: number, b: number) => a + (b - a) * random();
    const floor = (x0: number, x1: number, floorTop: number, z = 0, width = WIDE, kind: BlockKind = 'floor') => {
      seg.blocks.push({ kind, x0, x1, z, width, top: floorTop, height: kind === 'bridge' ? 40 : 260, theme });
    };
    const coinLine = (x0: number, x1: number, y: number, z = 0, count = 5) => {
      for (let i = 0; i < count; i++) seg.coins.push([x0 + (x1 - x0) * (i + 0.5) / count, y + 90, z]);
    };
    /** Gap before the next floor, clamped to reach. */
    const gap = (want: number) => Math.min(want, maxGap(difficulty));
    const rise = (want: number) => Math.max(-260, Math.min(want, maxRise(difficulty)));

    let x = start;
    let y = top;
    const table: Array<[string, number]> = index < 2
      ? [['run', 1]]
      : [['run', 1.2 - difficulty * 0.8], ['gap', 2], ['hurdles', 1.6], ['slalom', 1.2 + difficulty], ['narrow', 0.6 + difficulty * 1.4],
        ['stairs', 1], ['split', 0.4 + difficulty * 1.2], ['islands', 0.5 + difficulty * 1.5]];
    const total = table.reduce((sum, [, w]) => sum + w, 0);
    let roll = random() * total;
    let kind = table[0][0];
    for (const [name, weight] of table) { if ((roll -= weight) < 0) { kind = name; break; } }
    seg.kind = index === 0 ? 'start' : kind;

    switch (seg.kind) {
      case 'start': {
        floor(x - 1200, x + 2600, y);
        coinLine(x + 900, x + 2400, y, 0, 5);
        x += 2600;
        break;
      }
      case 'run': {
        const length = range(1600, 2400);
        floor(x, x + length, y);
        coinLine(x + 300, x + length - 300, y, Math.round(range(-1, 1)) * 300, 6);
        x += length;
        break;
      }
      case 'gap': {
        const g = gap(range(220, 300 + difficulty * 260));
        const r = rise(range(-120, 80 + difficulty * 80));
        // Coins arc over the gap to show the jump.
        for (let i = 0; i < 4; i++) {
          const t = (i + 0.5) / 4;
          seg.coins.push([x + g * t, y + Math.max(0, r) * t + 120 + Math.sin(t * Math.PI) * 180, 0]);
        }
        x += g;
        y += r;
        const length = range(1000, 1600);
        floor(x, x + length, y);
        x += length;
        break;
      }
      case 'hurdles': {
        const length = range(2000, 2800);
        floor(x, x + length, y);
        const count = 2 + Math.floor(random() * (2 + difficulty * 2));
        for (let i = 0; i < count; i++) {
          const hx = x + 600 + (length - 900) * (i + 0.5) / count;
          seg.blocks.push({ kind: 'hurdle', x0: hx - 30, x1: hx + 30, z: 0, width: WIDE, top: y + LIMITS.hurdle, height: LIMITS.hurdle, theme });
          seg.coins.push([hx, y + 300, 0]);
        }
        x += length;
        break;
      }
      case 'slalom': {
        const length = range(2200, 3000);
        floor(x, x + length, y);
        const count = 3 + Math.floor(random() * (2 + difficulty * 3));
        let side = random() < 0.5 ? -1 : 1;
        for (let i = 0; i < count; i++) {
          const px = x + 500 + (length - 800) * (i + 0.5) / count;
          // Each pillar blocks one half of the track; the coins mark the open half.
          seg.blocks.push({ kind: 'pillar', x0: px - 90, x1: px + 90, z: side * 300, width: 380, top: y + 380, height: 380, theme });
          seg.coins.push([px, y + 90, -side * 300]);
          side = -side;
        }
        x += length;
        break;
      }
      case 'narrow': {
        const length = range(1400, 2200);
        const width = lerp(520, 300, difficulty);
        const z = Math.round(range(-1, 1)) * 200;
        floor(x, x + length, y, z, width, 'bridge');
        coinLine(x + 200, x + length - 200, y, z, 7);
        x += length;
        break;
      }
      case 'stairs': {
        const steps = 3 + Math.floor(random() * 3);
        const up = random() < 0.65;
        for (let i = 0; i < steps; i++) {
          // Change height before each stair after the first, never after the last.
          if (i > 0) y += up ? rise(range(70, 130)) : -range(80, 160);
          const length = range(500, 700);
          floor(x, x + length, y);
          if (i % 2) seg.coins.push([x + length / 2, y + 90, 0]);
          x += length;
        }
        break;
      }
      case 'split': {
        // Two lanes over the void; pick one, then they merge again.
        const length = range(1600, 2400);
        const width = lerp(420, 320, difficulty);
        floor(x, x + length, y, -380, width, 'bridge');
        floor(x, x + length, y, 380, width, 'bridge');
        coinLine(x + 200, x + length - 200, y, random() < 0.5 ? -380 : 380, 6);
        x += length;
        break;
      }
      case 'islands': {
        const count = 3 + Math.floor(random() * 3);
        for (let i = 0; i < count; i++) {
          const g = gap(range(180, 260 + difficulty * 200));
          const r = rise(range(-90, 60 + difficulty * 60));
          seg.coins.push([x + g / 2, y + Math.max(r, 0) + 260, 0]);
          x += g;
          y += r;
          const length = range(420, 620 - difficulty * 120);
          floor(x, x + length, y, range(-1, 1) * 200, lerp(700, 460, difficulty));
          x += length;
        }
        break;
      }
    }
    seg.end = x;
    seg.exitTop = y;
    return seg;
  }
}

/** Every floor-to-floor transition along the centre-most path, for reachability tests. */
export function floorHops(segments: readonly Segment[]): Array<{ gap: number; rise: number; difficulty: number }> {
  const floors = segments.flatMap(seg => seg.blocks.filter(b => b.kind === 'floor' || b.kind === 'bridge').map(b => ({ b, d: seg.difficulty })));
  // Parallel split lanes share x-ranges; keep one per x-range.
  const chain: Array<{ b: Block; d: number }> = [];
  for (const entry of floors.sort((p, q) => p.b.x0 - q.b.x0)) {
    const last = chain[chain.length - 1];
    if (last && Math.abs(last.b.x0 - entry.b.x0) < 1) continue;
    chain.push(entry);
  }
  const hops: Array<{ gap: number; rise: number; difficulty: number }> = [];
  for (let i = 1; i < chain.length; i++) {
    hops.push({ gap: chain[i].b.x0 - chain[i - 1].b.x1, rise: chain[i].b.top - chain[i - 1].b.top, difficulty: Math.max(chain[i].d, chain[i - 1].d) });
  }
  return hops;
}
