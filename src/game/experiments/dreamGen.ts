/**
 * Mario's Infinite Dream — the pure, deterministic level generator.
 *
 * The world is an endless chain of "pieces" (set pieces such as island hops,
 * spiral stairs or checkerboards). Every piece starts where the last one
 * ended, so the route is continuous by construction; every gap and rise on
 * the route is clamped to a fraction of Mario's measured single-jump reach.
 */

/** Measured with MarioRuntime at full run speed (see tests/dreamGen.test.ts). */
export const MARIO_SINGLE_JUMP = { distance: 841, height: 335 } as const;
/** Route limits: comfortable at difficulty 0, demanding (but single-jumpable) at 1. */
export const ROUTE_LIMITS = {
  gap: [300, 470] as const,
  rise: [110, 185] as const,
};
export const BIOMES = [
  { name: 'Mushroom Meadow', from: 0 },
  { name: 'Sky Fortress', from: 9 },
  { name: 'Dreamscape', from: 19 },
] as const;

export type PlatformShape = 'island' | 'box' | 'pillar' | 'plank' | 'tile' | 'cube';
export type Surface = 'grass' | 'stone' | 'brick' | 'wood' | 'checker' | 'gold' | 'tiles' | 'cloud' | 'whiteBrick' | 'rainbow';

export interface Platform {
  shape: PlatformShape;
  x: number;
  /** Height of the standable top. */
  y: number;
  z: number;
  /** Diameter (round shapes) or width across the heading. */
  w: number;
  /** Depth along the yaw direction (round shapes: same as w). */
  d: number;
  /** Thickness below the top. */
  h: number;
  /** Radians; atan2(dx, dz) convention, so local +Z points along the heading. */
  yaw: number;
  surface: Surface;
  /** On the guaranteed route. */
  route: boolean;
  checkpoint?: boolean;
  tint?: number;
}

export type DecorKind = 'tree' | 'tower' | 'arch' | 'door' | 'inverted' | 'cube' | 'ruin' | 'ring' | 'post' | 'farIsland' | 'moon';
export interface Decor {
  kind: DecorKind;
  x: number;
  y: number;
  z: number;
  scale: number;
  yaw: number;
  tint?: number;
}

export interface CoinSpot { x: number; y: number; z: number }

export interface Piece {
  index: number;
  kind: string;
  biome: number;
  difficulty: number;
  platforms: Platform[];
  decor: Decor[];
  coins: CoinSpot[];
  /** Point on the last route platform's rim where the next piece begins. */
  exit: { x: number; y: number; z: number; heading: number };
  bounds: { minX: number; maxX: number; minY: number; maxY: number; minZ: number; maxZ: number };
}

export function rng(seed: number): () => number {
  let state = (seed >>> 0) || 0x9e3779b9;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function biomeFor(index: number): number {
  let biome = 0;
  BIOMES.forEach((entry, i) => { if (index >= entry.from) biome = i; });
  return biome;
}

export function difficultyFor(index: number): number {
  return Math.min(1, Math.max(0, (index - 1) / 26));
}

/** Half-extent of a platform along a horizontal unit direction. */
export function extentAlong(platform: Platform, dx: number, dz: number): number {
  if (platform.shape === 'island' || platform.shape === 'pillar') return platform.w / 2;
  const c = Math.cos(platform.yaw), s = Math.sin(platform.yaw);
  // Local axes: +Z = (sin yaw, cos yaw), +X = (cos yaw, -sin yaw).
  const alongZ = Math.abs(dx * s + dz * c);
  const alongX = Math.abs(dx * c - dz * s);
  return alongX * platform.w / 2 + alongZ * platform.d / 2;
}

/** Edge-to-edge horizontal gap and rise between two route platforms. */
export function hop(a: Platform, b: Platform): { gap: number; rise: number } {
  const dx = b.x - a.x, dz = b.z - a.z;
  const distance = Math.hypot(dx, dz);
  if (distance < 1e-6) return { gap: 0, rise: b.y - a.y };
  const ux = dx / distance, uz = dz / distance;
  return { gap: Math.max(0, distance - extentAlong(a, ux, uz) - extentAlong(b, ux, uz)), rise: b.y - a.y };
}

/** Largest gap allowed for a rise at a difficulty (higher rises shorten reach). */
export function maxGap(difficulty: number, rise: number): number {
  const base = ROUTE_LIMITS.gap[0] + (ROUTE_LIMITS.gap[1] - ROUTE_LIMITS.gap[0]) * difficulty;
  const climb = Math.max(0, rise) / MARIO_SINGLE_JUMP.height;
  return base * (1 - 0.45 * climb);
}
export function maxRise(difficulty: number): number {
  return ROUTE_LIMITS.rise[0] + (ROUTE_LIMITS.rise[1] - ROUTE_LIMITS.rise[0]) * difficulty;
}

interface Cursor { x: number; y: number; z: number; heading: number }

/** Deterministic, memoised chain of pieces. */
export class DreamGenerator {
  private readonly pieces: Piece[] = [];
  constructor(readonly seed: number, readonly origin: { x: number; y: number; z: number; heading: number }) {}

  piece(index: number): Piece {
    while (this.pieces.length <= index) this.pieces.push(this.build(this.pieces.length));
    return this.pieces[index];
  }

  /** Forget pieces far behind to bound memory; they regenerate identically from the chain. */
  get generated(): number { return this.pieces.length; }

  private build(index: number): Piece {
    const random = rng(this.seed * 7919 + index * 104729);
    const biome = biomeFor(index);
    const difficulty = difficultyFor(index);
    const previous = index > 0 ? this.pieces[index - 1] : null;
    const cursor: Cursor = previous ? { ...previous.exit } : { ...this.origin };
    const lastRoute = previous ? [...previous.platforms].reverse().find(p => p.route)! : null;
    const piece: Piece = {
      index, kind: 'start', biome, difficulty, platforms: [], decor: [], coins: [],
      exit: { ...cursor }, bounds: { minX: Infinity, maxX: -Infinity, minY: Infinity, maxY: -Infinity, minZ: Infinity, maxZ: -Infinity },
    };
    const builder = new PieceBuilder(piece, cursor, random, lastRoute);
    if (index === 0) builder.start();
    else if (index % 4 === 0) builder.checkpoint();
    else builder.setPiece(pickKind(biome, difficulty, random));
    piece.exit = { ...builder.cursor };
    builder.decorate();
    for (const p of piece.platforms) {
      const r = Math.max(p.w, p.d) / 2 + 100;
      piece.bounds.minX = Math.min(piece.bounds.minX, p.x - r);
      piece.bounds.maxX = Math.max(piece.bounds.maxX, p.x + r);
      piece.bounds.minZ = Math.min(piece.bounds.minZ, p.z - r);
      piece.bounds.maxZ = Math.max(piece.bounds.maxZ, p.z + r);
      piece.bounds.minY = Math.min(piece.bounds.minY, p.y - p.h);
      piece.bounds.maxY = Math.max(piece.bounds.maxY, p.y);
    }
    return piece;
  }
}

function pickKind(biome: number, difficulty: number, random: () => number): string {
  const table: Array<[string, number]> = biome === 0
    ? [['hop', 4], ['stairs', 2], ['bridge', 2], ['pillars', difficulty * 2]]
    : biome === 1
      ? [['pillars', 3], ['spiral', 3], ['stairs', 2], ['bridge', 1], ['hop', 1]]
      : [['checker', 3], ['cubes', 3], ['spiral', 2], ['ribbon', 2], ['pillars', 1]];
  const total = table.reduce((sum, [, w]) => sum + w, 0);
  let roll = random() * total;
  for (const [kind, weight] of table) {
    if ((roll -= weight) < 0) return kind;
  }
  return table[0][0];
}

const BIOME_SURFACES: Record<number, { main: Surface; alt: Surface; plank: Surface }> = {
  0: { main: 'grass', alt: 'stone', plank: 'wood' },
  1: { main: 'stone', alt: 'brick', plank: 'wood' },
  2: { main: 'checker', alt: 'tiles', plank: 'rainbow' },
};
const PASTELS = [0xffb3d9, 0xb3e5ff, 0xd4b3ff, 0xfff0a3, 0xb3ffd1];

class PieceBuilder {
  private last: Platform | null;
  constructor(private readonly piece: Piece, readonly cursor: Cursor, private readonly random: () => number, last: Platform | null) {
    this.last = last;
  }

  private get difficulty(): number { return this.piece.difficulty; }
  private get surfaces() { return BIOME_SURFACES[this.piece.biome]; }

  private range(a: number, b: number): number { return a + (b - a) * this.random(); }

  /** Turn gently, but keep the dream heading roughly forward overall. */
  private turn(amount = 0.55): void {
    const base = this.originHeading();
    let heading = this.cursor.heading + (this.random() - 0.5) * amount * 2;
    const drift = Math.atan2(Math.sin(heading - base), Math.cos(heading - base));
    if (Math.abs(drift) > 0.85) heading = base + Math.sign(drift) * 0.85;
    this.cursor.heading = heading;
  }

  private originHeading(): number { return Math.PI / 2; }

  /**
   * Place the next route platform beyond the cursor along the heading,
   * clamping gap and rise to Mario's reach at the current difficulty.
   */
  private next(template: Omit<Platform, 'x' | 'y' | 'z' | 'route' | 'yaw'> & { yaw?: number }, gap: number, rise: number): Platform {
    const difficulty = this.difficulty;
    rise = Math.min(rise, maxRise(difficulty));
    gap = Math.max(0, Math.min(gap, maxGap(difficulty, rise) * 0.97));
    const heading = this.cursor.heading;
    const dx = Math.sin(heading), dz = Math.cos(heading);
    const yaw = template.yaw ?? heading;
    const probe: Platform = { ...template, x: 0, y: 0, z: 0, yaw, route: true };
    const half = extentAlong(probe, dx, dz);
    const platform: Platform = {
      ...probe,
      x: this.cursor.x + dx * (gap + half),
      y: this.cursor.y + rise,
      z: this.cursor.z + dz * (gap + half),
    };
    // Verify against the true previous platform (its rim may not be where the cursor sits).
    if (this.last) {
      for (let i = 0; i < 8; i++) {
        const check = hop(this.last, platform);
        const limit = maxGap(difficulty, check.rise);
        if (check.gap <= limit) break;
        const pull = (check.gap - limit) + 10;
        platform.x -= dx * pull;
        platform.z -= dz * pull;
      }
    }
    this.piece.platforms.push(platform);
    this.last = platform;
    this.cursor.x = platform.x + dx * half;
    this.cursor.z = platform.z + dz * half;
    this.cursor.y = platform.y;
    return platform;
  }

  /** Move the cursor to the rim of the last platform in the current heading. */
  private reaim(): void {
    if (!this.last) return;
    const dx = Math.sin(this.cursor.heading), dz = Math.cos(this.cursor.heading);
    const half = extentAlong(this.last, dx, dz);
    this.cursor.x = this.last.x + dx * half;
    this.cursor.z = this.last.z + dz * half;
  }

  private coinArc(a: Platform, b: Platform, count = 3): void {
    for (let i = 1; i <= count; i++) {
      const t = i / (count + 1);
      this.piece.coins.push({
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t + 120 + Math.sin(t * Math.PI) * 160,
        z: a.z + (b.z - a.z) * t,
      });
    }
  }

  start(): void {
    this.piece.kind = 'start';
    const p: Platform = { shape: 'island', x: this.cursor.x, y: this.cursor.y, z: this.cursor.z, w: 1300, d: 1300, h: 90, yaw: 0, surface: 'grass', route: true, checkpoint: true };
    this.piece.platforms.push(p);
    this.last = p;
    this.reaim();
  }

  checkpoint(): void {
    this.piece.kind = 'checkpoint';
    this.turn(0.3);
    const size = this.range(820, 1000);
    const p = this.next({ shape: 'island', w: size, d: size, h: 90, surface: this.surfaces.main, checkpoint: true }, this.range(160, 240), this.range(-60, 60));
    if (this.piece.biome === 2) p.tint = PASTELS[this.piece.index % PASTELS.length];
    this.turn(0.4);
    this.reaim();
  }

  setPiece(kind: string): void {
    this.piece.kind = kind;
    const d = this.difficulty;
    const s = this.surfaces;
    switch (kind) {
      case 'hop': {
        const count = 3 + Math.floor(this.random() * 3);
        for (let i = 0; i < count; i++) {
          this.turn(0.35);
          this.reaim();
          const size = this.range(520 - d * 200, 700 - d * 220);
          const before = this.last;
          const p = this.next({ shape: 'island', w: size, d: size, h: 70, surface: s.main }, this.range(170, 260 + d * 160), this.range(-80, 70 + d * 90));
          if (before) this.coinArc(before, p, 3);
        }
        break;
      }
      case 'stairs': {
        const steps = 5 + Math.floor(this.random() * 4);
        const size = this.range(300, 380) - d * 60;
        for (let i = 0; i < steps; i++) {
          if (i % 3 === 2) { this.turn(0.25); this.reaim(); }
          this.next({ shape: 'box', w: size, d: size, h: 160, surface: i % 2 ? s.alt : s.main }, this.range(90, 150 + d * 110), this.range(70, 100 + d * 60));
        }
        break;
      }
      case 'pillars': {
        const count = 4 + Math.floor(this.random() * 3);
        for (let i = 0; i < count; i++) {
          this.turn(0.45);
          this.reaim();
          const radius = this.range(260, 340) - d * 80;
          const before = this.last;
          const p = this.next({ shape: 'pillar', w: radius, d: radius, h: this.range(900, 2400), surface: s.alt }, this.range(180, 270 + d * 150), this.range(-60, 50 + d * 100));
          if (before && i % 2) this.coinArc(before, p, 2);
        }
        break;
      }
      case 'bridge':
      case 'ribbon': {
        // A continuous, curving plank path; ribbons climb and twist.
        const planks = 10 + Math.floor(this.random() * 6);
        const width = kind === 'ribbon' ? 210 - d * 40 : 260 - d * 60;
        const curl = (this.random() - 0.5) * (kind === 'ribbon' ? 0.5 : 0.24);
        for (let i = 0; i < planks; i++) {
          this.cursor.heading += curl;
          this.reaim();
          const p = this.next({ shape: 'plank', w: width, d: 150, h: 30, surface: s.plank, tint: kind === 'ribbon' ? PASTELS[i % PASTELS.length] : undefined }, 0, kind === 'ribbon' ? 34 : this.range(-6, 12));
          if (i % 3 === 1) this.piece.coins.push({ x: p.x, y: p.y + 110, z: p.z });
        }
        break;
      }
      case 'spiral': {
        // Steps wind around a tall central tower (non-route collision).
        const radius = this.range(560, 680);
        const turnSign = this.random() < 0.5 ? 1 : -1;
        const dx = Math.sin(this.cursor.heading), dz = Math.cos(this.cursor.heading);
        const startAngle = Math.atan2(-dx, -dz);
        const center = { x: this.cursor.x + dx * (radius + 200), z: this.cursor.z + dz * (radius + 200) };
        const steps = 9 + Math.floor(this.random() * 4);
        const stepArc = (1.6 * Math.PI) / steps;
        const towerTop = this.cursor.y + steps * 80 + 260;
        this.piece.platforms.push({ shape: 'pillar', x: center.x, y: towerTop, z: center.z, w: radius * 0.9, d: radius * 0.9, h: towerTop - this.cursor.y + 1400, yaw: 0, surface: this.surfaces.alt, route: false });
        for (let i = 0; i < steps; i++) {
          const a = startAngle + turnSign * i * stepArc;
          const px = center.x + Math.sin(a) * radius;
          const pz = center.z + Math.cos(a) * radius;
          // Aim the cursor at the next step so gap/rise clamping applies to the spiral too.
          const heading = Math.atan2(px - this.cursor.x, pz - this.cursor.z);
          this.cursor.heading = heading;
          this.reaim();
          const toward = Math.hypot(px - this.cursor.x, pz - this.cursor.z);
          const size = 330 - d * 60;
          this.next({ shape: 'box', w: size, d: size, h: 70, surface: i % 2 ? this.surfaces.main : this.surfaces.alt, yaw: a }, Math.max(0, toward - size / 2), this.range(70, 95 + d * 40));
        }
        this.piece.decor.push({ kind: 'ring', x: center.x, y: towerTop + 260, z: center.z, scale: 1, yaw: 0 });
        this.cursor.heading = this.originHeading() + (this.random() - 0.5) * 0.6;
        this.reaim();
        break;
      }
      case 'checker': {
        // A floating checkerboard with holes; one tile per row is always solid.
        const rows = 5 + Math.floor(this.random() * 3);
        const tile = 300 - d * 40;
        const heading = this.cursor.heading;
        const side = { x: Math.cos(heading), z: -Math.sin(heading) };
        for (let row = 0; row < rows; row++) {
          this.reaim();
          const main = this.next({ shape: 'tile', w: tile, d: tile, h: 40, surface: 'checker', tint: PASTELS[row % PASTELS.length] }, this.range(40, 120 + d * 150), this.range(-20, 50));
          for (const offset of [-1, 1]) {
            if (this.random() < 0.35 + d * 0.3) continue;
            this.piece.platforms.push({ ...main, x: main.x + side.x * offset * (tile + 30), z: main.z + side.z * offset * (tile + 30), route: false, tint: PASTELS[(row + offset + 5) % PASTELS.length] });
          }
        }
        break;
      }
      case 'cubes': {
        // Big tilted-looking cubes floating at odd heights.
        const count = 4 + Math.floor(this.random() * 3);
        for (let i = 0; i < count; i++) {
          this.turn(0.6);
          this.reaim();
          const size = this.range(360, 460) - d * 70;
          const before = this.last;
          const p = this.next({ shape: 'cube', w: size, d: size, h: size, surface: 'gold', yaw: this.random() * Math.PI, tint: PASTELS[i % PASTELS.length] }, this.range(170, 260 + d * 160), this.range(-90, 80 + d * 100));
          if (before) this.coinArc(before, p, 2);
        }
        break;
      }
      default:
        this.checkpoint();
    }
    this.turn(0.3);
    this.reaim();
  }

  decorate(): void {
    const piece = this.piece;
    const random = this.random;
    for (const p of piece.platforms) {
      if (p.shape === 'island' && piece.biome === 0) {
        const trees = p.checkpoint ? 3 : this.random() < 0.6 ? 1 : 0;
        for (let i = 0; i < trees; i++) {
          // Trees grow beside the route, never on it: perpendicular to the heading.
          const side = random() < 0.5 ? 1 : -1;
          const a = piece.exit.heading + side * (Math.PI / 2 + (random() - 0.5) * 0.9), r = (0.28 + random() * 0.12) * p.w;
          piece.decor.push({ kind: 'tree', x: p.x + Math.sin(a) * r, y: p.y, z: p.z + Math.cos(a) * r, scale: 0.8 + random() * 0.6, yaw: random() * 6 });
        }
      }
      if (p.checkpoint) piece.decor.push({ kind: piece.biome === 1 ? 'arch' : piece.biome === 2 ? 'door' : 'post', x: p.x, y: p.y, z: p.z, scale: 1, yaw: piece.exit.heading });
    }
    // Background architecture grows stranger with each biome.
    const along = { x: Math.sin(piece.exit.heading), z: Math.cos(piece.exit.heading) };
    const side = { x: along.z, z: -along.x };
    const count = 2 + Math.floor(random() * 2);
    for (let i = 0; i < count; i++) {
      const s = random() < 0.5 ? -1 : 1;
      const out = 2600 + random() * 5200;
      const x = piece.exit.x + side.x * s * out - along.x * random() * 1500;
      const z = piece.exit.z + side.z * s * out - along.z * random() * 1500;
      const y = piece.exit.y + (random() - 0.45) * 2600;
      const kind: DecorKind = piece.biome === 0 ? 'farIsland'
        : piece.biome === 1 ? (random() < 0.6 ? 'tower' : 'ruin')
          : (['inverted', 'cube', 'door', 'inverted'] as const)[Math.floor(random() * 4)];
      piece.decor.push({ kind, x, y, z, scale: 0.8 + random() * 1.8, yaw: random() * Math.PI * 2, tint: PASTELS[Math.floor(random() * PASTELS.length)] });
    }
    if (piece.biome === 2 && piece.index % 3 === 0) {
      piece.decor.push({ kind: 'inverted', x: piece.exit.x, y: piece.exit.y + 1700 + random() * 600, z: piece.exit.z, scale: 1.4, yaw: random() * 6, tint: PASTELS[piece.index % PASTELS.length] });
    }
  }
}
