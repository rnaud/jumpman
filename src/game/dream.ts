import * as THREE from 'three';
import type { CollisionWorld, Surface } from '../sim/collision';

const CELL = 620;
const RADIUS = 2;

interface Island {
  mesh: THREE.Mesh;
  surfaces: Surface[];
}

/** Deterministic procedural island streamer for Mario's Infinite Dream. */
export class DreamWorld {
  enabled = false;
  private readonly islands = new Map<string, Island[]>();
  private readonly palette = [0x7ed6a6, 0x74c7e8, 0xd9b4f0, 0xf2c879, 0x9fdcb7];
  private readonly geometry = new THREE.BoxGeometry(1, 1, 1);
  private lastCell = '';

  constructor(
    private readonly scene: THREE.Scene,
    private world: CollisionWorld,
    private readonly seed = 1,
  ) {}

  setWorld(world: CollisionWorld): void {
    this.disable();
    this.world = world;
  }

  disable(): void {
    this.enabled = false;
    this.clear();
    this.lastCell = '';
  }

  toggle(position: [number, number, number]): boolean {
    this.enabled = !this.enabled;
    if (this.enabled) {
      this.lastCell = '';
      this.stream(position);
    } else {
      this.disable();
    }
    return this.enabled;
  }

  update(position: [number, number, number]): void {
    if (!this.enabled) return;
    const cell = `${Math.floor(position[0] / CELL)},${Math.floor(position[2] / CELL)}`;
    if (cell === this.lastCell) return;
    this.lastCell = cell;
    this.stream(position);
  }

  debug(): { enabled: boolean; islands: number } {
    let count = 0;
    for (const list of this.islands.values()) count += list.length;
    return { enabled: this.enabled, islands: count };
  }

  private stream(position: [number, number, number]): void {
    const cx = Math.floor(position[0] / CELL);
    const cz = Math.floor(position[2] / CELL);
    for (let x = cx - RADIUS; x <= cx + RADIUS; x++) {
      for (let z = cz - RADIUS; z <= cz + RADIUS; z++) {
        const key = `${x},${z}`;
        if (this.islands.has(key)) continue;
        this.createCell(key, x, z);
      }
    }
    for (const [key, list] of this.islands) {
      const [x, z] = key.split(',').map(Number);
      if (Math.abs(x - cx) > RADIUS + 1 || Math.abs(z - cz) > RADIUS + 1) {
        for (const island of list) this.removeIsland(island);
        this.islands.delete(key);
      }
    }
  }

  private createCell(key: string, x: number, z: number): void {
    const random = this.makeRng(this.seed + x * 73856093 + z * 19349663);
    const list: Island[] = [];
    const count = 1 + (random() > 0.55 ? 1 : 0);
    for (let i = 0; i < count; i++) {
      const width = 260 + random() * 260;
      const depth = 260 + random() * 260;
      const height = 32 + random() * 28;
      const px = x * CELL + (random() - 0.5) * CELL * 0.55 + (i === 1 ? 260 : 0);
      const pz = z * CELL + (random() - 0.5) * CELL * 0.55;
      const py = 100 + random() * 360 + (i === 1 ? 130 : 0);
      list.push(this.createIsland(px, py, pz, width, height, depth, random));
    }
    this.islands.set(key, list);
  }

  private createIsland(x: number, y: number, z: number, width: number, height: number, depth: number, random: () => number): Island {
    const mesh = new THREE.Mesh(this.geometry, new THREE.MeshStandardMaterial({
      color: this.palette[Math.floor(random() * this.palette.length)],
      roughness: 0.8,
      metalness: 0.02,
    }));
    mesh.scale.set(width, height, depth);
    mesh.position.set(x, y, z);
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    this.scene.add(mesh);

    const hx = width / 2;
    const hy = height / 2;
    const hz = depth / 2;
    const top = y + hy;
    const bottom = y - hy;
    const surfaces: Surface[] = [];
    const add = (a: [number, number, number], b: [number, number, number], c: [number, number, number], d: [number, number, number]) => {
      surfaces.push(...this.world.addQuad(a, b, c, d));
    };
    add([x - hx, top, z - hz], [x - hx, top, z + hz], [x + hx, top, z + hz], [x + hx, top, z - hz]);
    add([x - hx, bottom, z - hz], [x + hx, bottom, z - hz], [x + hx, bottom, z + hz], [x - hx, bottom, z + hz]);
    add([x - hx, top, z - hz], [x - hx, top, z + hz], [x - hx, bottom, z + hz], [x - hx, bottom, z - hz]);
    add([x + hx, top, z - hz], [x + hx, top, z + hz], [x + hx, bottom, z + hz], [x + hx, bottom, z - hz]);
    add([x + hx, top, z + hz], [x - hx, top, z + hz], [x - hx, bottom, z + hz], [x + hx, bottom, z + hz]);
    add([x - hx, top, z - hz], [x + hx, top, z - hz], [x + hx, bottom, z - hz], [x - hx, bottom, z - hz]);
    return { mesh, surfaces };
  }

  private removeIsland(island: Island): void {
    this.world.removeSurfaces(new Set(island.surfaces));
    this.scene.remove(island.mesh);
    (island.mesh.material as THREE.Material).dispose();
  }

  private clear(): void {
    for (const list of this.islands.values()) for (const island of list) this.removeIsland(island);
    this.islands.clear();
  }

  private makeRng(seed: number): () => number {
    let state = seed >>> 0;
    return () => {
      state = (state * 1664525 + 1013904223) >>> 0;
      return state / 0xffffffff;
    };
  }
}
