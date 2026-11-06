/**
 * Triangle collision world, ported from src/engine/surface_collision.c.
 *
 * SM64 keeps a 16x16 grid of 1024-unit cells and stores wall, floor and
 * ceiling triangles separately.  Floors are sorted highest-first by their
 * first vertex, ceilings lowest-first, and walls in insertion order.  The
 * "first triangle that matches wins" behaviour is load-bearing: it is why
 * SM64 can find a lower floor before a higher one ("surface cucking").
 *
 * We keep the same partition and ordering, but use a sparse map instead of a
 * fixed 16x16 array so the playground can grow.
 */

import { Vec3 } from './math';
import {
  SURFACE_CAMERA_BOUNDARY,
  SURFACE_FLAG_X_PROJECTION,
  SURFACE_VANISH_CAP_WALLS,
} from './surfaces';

export type SurfaceType = number;

export interface Surface {
  type: SurfaceType;
  force: number;
  flags: number;
  lowerY: number;
  upperY: number;
  v1: Vec3;
  v2: Vec3;
  v3: Vec3;
  normal: Vec3;
  originOffset: number;
}

export interface SurfaceOptions {
  type?: SurfaceType;
  force?: number;
  flags?: number;
}

export interface FloorHit {
  height: number;
  surface: Surface | null;
}

export interface CeilHit {
  height: number;
  surface: Surface | null;
}

export interface WallCollisionData {
  x: number;
  y: number;
  z: number;
  offsetY: number;
  radius: number;
  numWalls: number;
  walls: Surface[];
}

interface Cell {
  floors: Surface[];
  ceils: Surface[];
  walls: Surface[];
  dirty: boolean;
}

export const LEVEL_BOUNDARY_MAX = 0x2000;
export const CELL_SIZE = 1 << 10;
export const CELL_HEIGHT_LIMIT = 20000;
export const FLOOR_LOWER_LIMIT = -11000;

const MAX_WALLS = 4;

export class CollisionWorld {
  readonly cellSize: number;
  waterBoxes: [number, number, number, number, number, number][] = [];

  waterLevel(x: number, z: number): number {
    for (const [id, x1, z1, x2, z2, y] of this.waterBoxes) {
      if (id < 50 && x > x1 && x < x2 && z > z1 && z < z2) return y;
    }
    return -Infinity;
  }

  snowTerrain = false;
  private readonly cells = new Map<number, Cell>();
  private allSurfaces: Surface[] = [];

  constructor(cellSize = CELL_SIZE) {
    this.cellSize = cellSize;
  }

  get surfaces(): readonly Surface[] {
    return this.allSurfaces;
  }

  clear(): void {
    this.cells.clear();
    this.allSurfaces.length = 0;
  }

  removeSurfaces(removed: ReadonlySet<Surface>): void {
    if (!removed.size) return;
    // Dynamic platforms are reinserted in their new cells before Mario steps.
    const touched = new Set<Cell>();
    for (const s of removed) {
      const x0 = Math.floor((Math.min(s.v1[0], s.v2[0], s.v3[0]) - 50) / this.cellSize);
      const x1 = Math.floor((Math.max(s.v1[0], s.v2[0], s.v3[0]) + 50) / this.cellSize);
      const z0 = Math.floor((Math.min(s.v1[2], s.v2[2], s.v3[2]) - 50) / this.cellSize);
      const z1 = Math.floor((Math.max(s.v1[2], s.v2[2], s.v3[2]) + 50) / this.cellSize);
      for (let x = x0; x <= x1; x++) for (let z = z0; z <= z1; z++) {
        const cell = this.getCell(x, z);
        if (cell) touched.add(cell);
      }
    }
    for (const cell of touched) {
      cell.floors = cell.floors.filter(s => !removed.has(s));
      cell.walls = cell.walls.filter(s => !removed.has(s));
      cell.ceils = cell.ceils.filter(s => !removed.has(s));
    }
    this.allSurfaces = this.allSurfaces.filter(s => !removed.has(s));
  }

  private cellKey(cx: number, cz: number): number {
    // Coordinates are small in practice; pack two signed 16-bit cell coords.
    return ((cx + 32768) << 16) ^ (cz + 32768);
  }

  private getOrCreateCell(cx: number, cz: number): Cell {
    const key = this.cellKey(cx, cz);
    let cell = this.cells.get(key);
    if (!cell) {
      cell = { floors: [], ceils: [], walls: [], dirty: false };
      this.cells.set(key, cell);
    }
    return cell;
  }

  private getCell(cx: number, cz: number): Cell | undefined {
    return this.cells.get(this.cellKey(cx, cz));
  }

  /**
   * Add one triangle using the decompilation's normal convention:
   * n = (v2 - v1) x (v3 - v2), normalised.
   */
  addTriangle(v1: Vec3, v2: Vec3, v3: Vec3, options: SurfaceOptions = {}): Surface | null {
    const [x1, y1, z1] = v1;
    const [x2, y2, z2] = v2;
    const [x3, y3, z3] = v3;

    let nx = (y2 - y1) * (z3 - z2) - (z2 - z1) * (y3 - y2);
    let ny = (z2 - z1) * (x3 - x2) - (x2 - x1) * (z3 - z2);
    let nz = (x2 - x1) * (y3 - y2) - (y2 - y1) * (x3 - x2);
    const mag = Math.sqrt(nx * nx + ny * ny + nz * nz);
    if (mag < 0.0001) return null;
    const inv = 1 / mag;
    nx *= inv;
    ny *= inv;
    nz *= inv;

    let flags = options.flags ?? 0;
    let listIndex: 'floors' | 'ceils' | 'walls';
    if (ny > 0.01) {
      listIndex = 'floors';
    } else if (ny < -0.01) {
      listIndex = 'ceils';
    } else {
      listIndex = 'walls';
      if (nx < -0.707 || nx > 0.707) flags |= SURFACE_FLAG_X_PROJECTION;
    }

    const lowerY = Math.min(y1, y2, y3) - 5;
    const upperY = Math.max(y1, y2, y3) + 5;

    const surface: Surface = {
      type: options.type ?? 0,
      force: options.force ?? 0,
      flags,
      lowerY,
      upperY,
      v1: [x1, y1, z1],
      v2: [x2, y2, z2],
      v3: [x3, y3, z3],
      normal: [nx, ny, nz],
      originOffset: -(nx * x1 + ny * y1 + nz * z1),
    };

    // SM64 expands a surface's cell range by one cell when it sits within 50
    // units of a cell boundary (lower_cell_index / upper_cell_index).  This
    // matters for wall collision radii, which are also 50 units.
    const CELL_PAD = 50;
    const minX = Math.floor((Math.min(x1, x2, x3) - CELL_PAD) / this.cellSize);
    const maxX = Math.floor((Math.max(x1, x2, x3) + CELL_PAD) / this.cellSize);
    const minZ = Math.floor((Math.min(z1, z2, z3) - CELL_PAD) / this.cellSize);
    const maxZ = Math.floor((Math.max(z1, z2, z3) + CELL_PAD) / this.cellSize);

    for (let cz = minZ; cz <= maxZ; cz++) {
      for (let cx = minX; cx <= maxX; cx++) {
        const cell = this.getOrCreateCell(cx, cz);
        cell[listIndex].push(surface);
        cell.dirty = true;
      }
    }

    this.allSurfaces.push(surface);
    return surface;
  }

  addQuad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, options: SurfaceOptions = {}): Surface[] {
    const out: Surface[] = [];
    const s1 = this.addTriangle(a, b, c, options);
    const s2 = this.addTriangle(a, c, d, options);
    if (s1) out.push(s1);
    if (s2) out.push(s2);
    return out;
  }

  /** Sort every cell's floors and ceilings the way SM64 does. */
  finalize(): void {
    for (const cell of this.cells.values()) this.sortCell(cell);
  }

  private sortCell(cell: Cell): void {
    if (!cell.dirty) return;
    // Highest first, insertion order preserved for ties (JS sort is stable).
    cell.floors.sort((a, b) => b.v1[1] - a.v1[1]);
    // Lowest first.
    cell.ceils.sort((a, b) => a.v1[1] - b.v1[1]);
    cell.dirty = false;
  }

  private cellForQuery(x: number, z: number): Cell | undefined {
    const cell = this.getCell(Math.floor(x / this.cellSize), Math.floor(z / this.cellSize));
    if (cell?.dirty) this.sortCell(cell);
    return cell;
  }

  /**
   * Find the first floor below (or just above) a point.
   * Mirrors find_floor in the decompilation, including its 78-unit buffer and
   * its use of truncated integer coordinates for the triangle test.
   */
  findFloor(x: number, y: number, z: number): FloorHit {
    const qx = Math.trunc(x);
    const qy = Math.trunc(y);
    const qz = Math.trunc(z);
    const cell = this.cellForQuery(qx, qz);
    if (!cell) return { height: FLOOR_LOWER_LIMIT, surface: null };

    for (const surf of cell.floors) {
      if (this.isCameraOnly(surf)) continue;

      const x1 = Math.trunc(surf.v1[0]);
      const z1 = Math.trunc(surf.v1[2]);
      const x2 = Math.trunc(surf.v2[0]);
      const z2 = Math.trunc(surf.v2[2]);

      if ((z1 - qz) * (x2 - x1) - (x1 - qx) * (z2 - z1) < 0) continue;

      const x3 = Math.trunc(surf.v3[0]);
      const z3 = Math.trunc(surf.v3[2]);
      if ((z2 - qz) * (x3 - x2) - (x2 - qx) * (z3 - z2) < 0) continue;
      if ((z3 - qz) * (x1 - x3) - (x3 - qx) * (z1 - z3) < 0) continue;

      const nx = surf.normal[0];
      const ny = surf.normal[1];
      const nz = surf.normal[2];
      if (ny === 0) continue;

      const height = -(qx * nx + nz * qz + surf.originOffset) / ny;
      if (qy - (height - 78) < 0) continue;

      return { height, surface: surf };
    }

    return { height: FLOOR_LOWER_LIMIT, surface: null };
  }

  findFloorHeight(x: number, y: number, z: number): number {
    return this.findFloor(x, y, z).height;
  }

  /** Mirrors find_ceil: first ceiling at least 78 units above the point. */
  findCeil(x: number, y: number, z: number): CeilHit {
    const qx = Math.trunc(x);
    const qy = Math.trunc(y);
    const qz = Math.trunc(z);
    const cell = this.cellForQuery(qx, qz);
    if (!cell) return { height: CELL_HEIGHT_LIMIT, surface: null };

    for (const surf of cell.ceils) {
      if (this.isCameraOnly(surf)) continue;

      const x1 = Math.trunc(surf.v1[0]);
      const z1 = Math.trunc(surf.v1[2]);
      const x2 = Math.trunc(surf.v2[0]);
      const z2 = Math.trunc(surf.v2[2]);

      if ((z1 - qz) * (x2 - x1) - (x1 - qx) * (z2 - z1) > 0) continue;

      const x3 = Math.trunc(surf.v3[0]);
      const z3 = Math.trunc(surf.v3[2]);
      if ((z2 - qz) * (x3 - x2) - (x2 - qx) * (z3 - z2) > 0) continue;
      if ((z3 - qz) * (x1 - x3) - (x3 - qx) * (z1 - z3) > 0) continue;

      const nx = surf.normal[0];
      const ny = surf.normal[1];
      const nz = surf.normal[2];
      if (ny === 0) continue;

      const height = -(qx * nx + nz * qz + surf.originOffset) / ny;
      if (qy - (height + 78) > 0) continue;

      return { height, surface: surf };
    }

    return { height: CELL_HEIGHT_LIMIT, surface: null };
  }

  private isCameraOnly(surf: Surface): boolean {
    return surf.type === SURFACE_CAMERA_BOUNDARY;
  }

  /**
   * Push a cylinder (radius, offsetY at its base) out of every wall it touches.
   * This is find_wall_collisions + find_wall_collisions_from_list.
   */
  findWallCollisions(data: WallCollisionData): number {
    data.numWalls = 0;
    const radius = Math.min(data.radius, 200);
    const cell = this.getCell(Math.floor(data.x / this.cellSize), Math.floor(data.z / this.cellSize));
    if (!cell) return 0;

    let numCols = 0;
    const y = data.y + data.offsetY;

    for (const surf of cell.walls) {
      if (y < surf.lowerY || y > surf.upperY) continue;

      let offset =
        surf.normal[0] * data.x + surf.normal[1] * y + surf.normal[2] * data.z + surf.originOffset;
      if (offset < -radius || offset > radius) continue;

      // Ignore camera-only and vanish-cap surfaces for Mario.
      if (
        surf.type === SURFACE_CAMERA_BOUNDARY ||
        surf.type === SURFACE_VANISH_CAP_WALLS
      ) {
        continue;
      }

      const px = data.x;
      const pz = data.z;
      let w1: number, w2: number, w3: number;
      let y1: number, y2: number, y3: number;

      if (surf.flags & SURFACE_FLAG_X_PROJECTION) {
        w1 = -surf.v1[2];
        w2 = -surf.v2[2];
        w3 = -surf.v3[2];
        y1 = surf.v1[1];
        y2 = surf.v2[1];
        y3 = surf.v3[1];

        if (surf.normal[0] > 0) {
          if ((y1 - y) * (w2 - w1) - (w1 - -pz) * (y2 - y1) > 0) continue;
          if ((y2 - y) * (w3 - w2) - (w2 - -pz) * (y3 - y2) > 0) continue;
          if ((y3 - y) * (w1 - w3) - (w3 - -pz) * (y1 - y3) > 0) continue;
        } else {
          if ((y1 - y) * (w2 - w1) - (w1 - -pz) * (y2 - y1) < 0) continue;
          if ((y2 - y) * (w3 - w2) - (w2 - -pz) * (y3 - y2) < 0) continue;
          if ((y3 - y) * (w1 - w3) - (w3 - -pz) * (y1 - y3) < 0) continue;
        }
      } else {
        w1 = surf.v1[0];
        w2 = surf.v2[0];
        w3 = surf.v3[0];
        y1 = surf.v1[1];
        y2 = surf.v2[1];
        y3 = surf.v3[1];

        if (surf.normal[2] > 0) {
          if ((y1 - y) * (w2 - w1) - (w1 - px) * (y2 - y1) > 0) continue;
          if ((y2 - y) * (w3 - w2) - (w2 - px) * (y3 - y2) > 0) continue;
          if ((y3 - y) * (w1 - w3) - (w3 - px) * (y1 - y3) > 0) continue;
        } else {
          if ((y1 - y) * (w2 - w1) - (w1 - px) * (y2 - y1) < 0) continue;
          if ((y2 - y) * (w3 - w2) - (w2 - px) * (y3 - y2) < 0) continue;
          if ((y3 - y) * (w1 - w3) - (w3 - px) * (y1 - y3) < 0) continue;
        }
      }

      data.x += surf.normal[0] * (radius - offset);
      data.z += surf.normal[2] * (radius - offset);

      if (data.numWalls < MAX_WALLS) {
        data.walls[data.numWalls++] = surf;
      }
      numCols++;
    }

    return numCols;
  }

  f32FindWallCollision(pos: Vec3, offsetY: number, radius: number): number {
    const data: WallCollisionData = {
      x: pos[0],
      y: pos[1],
      z: pos[2],
      offsetY,
      radius,
      numWalls: 0,
      walls: [],
    };
    const count = this.findWallCollisions(data);
    pos[0] = data.x;
    pos[1] = data.y;
    pos[2] = data.z;
    return count;
  }

  /** Wall push that also returns the most recent wall (used by ground/air steps). */
  resolveAndReturnWallCollisions(pos: Vec3, offsetY: number, radius: number): Surface | null {
    const data: WallCollisionData = {
      x: pos[0],
      y: pos[1],
      z: pos[2],
      offsetY,
      radius,
      numWalls: 0,
      walls: [],
    };
    let wall: Surface | null = null;
    if (this.findWallCollisions(data) && data.numWalls > 0) {
      wall = data.walls[data.numWalls - 1];
    }
    pos[0] = data.x;
    pos[1] = data.y;
    pos[2] = data.z;
    return wall;
  }

  vec3fFindCeil(pos: Vec3, height: number): CeilHit {
    return this.findCeil(pos[0], height + 80, pos[2]);
  }
}

/**
 * Floor height relative to Mario in a polar direction, from mario.c.
 */
export function findFloorHeightRelativePolar(
  world: CollisionWorld,
  x: number,
  y: number,
  z: number,
  faceYaw: number,
  angleFromMario: number,
  distFromMario: number,
  sin: (a: number) => number,
  cos: (a: number) => number,
): number {
  const a = faceYaw + angleFromMario;
  const dz = sin(a) * distFromMario;
  const dx = cos(a) * distFromMario;
  return world.findFloor(x + dz, y + 100, z + dx).height;
}
