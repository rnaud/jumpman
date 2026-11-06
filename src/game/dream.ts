import * as THREE from 'three';
import type { CollisionWorld, Surface } from '../sim/collision';
import { instantiateSm64, loadSm64Asset, SM64_MODELS } from './sm64Assets';

const CELL = 620;
const RADIUS = 2;

interface Island {
  mesh: THREE.Mesh;
  surfaces: Surface[];
  props: THREE.Object3D[];
}

/** Deterministic procedural island streamer for Mario's Infinite Dream. */
export class DreamWorld {
  enabled = false;
  private readonly islands = new Map<string, Island[]>();
  private readonly allSurfaces = new Set<Surface>();
  private readonly mains = new Map<string, { x: number; y: number; z: number; top: number; width: number; depth: number; biome: number; difficulty: number }>();
  private readonly biomes = [
    [0x7ed6a6, 0x9fdcb7],
    [0x74c7e8, 0x7f9ff0],
    [0xd9b4f0, 0xf2c879],
  ];
  private readonly geometry = new THREE.BoxGeometry(1, 1, 1);
  private lastCell = '';
  private lastPosition: [number, number, number] = [0, 0, 0];
  private treeTemplate: Awaited<ReturnType<typeof loadSm64Asset>> | null = null;
  private coinTemplate: Awaited<ReturnType<typeof loadSm64Asset>> | null = null;
  private preparing = false;

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
    this.lastPosition = [...position];
    if (this.enabled) {
      this.lastCell = '';
      this.stream(position);
      void this.prepare();
    } else {
      this.disable();
    }
    return this.enabled;
  }

  update(position: [number, number, number]): void {
    if (!this.enabled) return;
    this.lastPosition = [...position];
    const cell = `${Math.floor(position[0] / CELL)},${Math.floor(position[2] / CELL)}`;
    if (cell === this.lastCell) return;
    this.lastCell = cell;
    this.stream(position);
  }

  ownsSurface(surface: Surface | null): boolean {
    return surface !== null && this.allSurfaces.has(surface);
  }

  debugPath(): Array<{ x: number; y: number; z: number; top: number; width: number; depth: number; biome: number; difficulty: number }> {
    return [...this.mains.values()].sort((a, b) => a.z - b.z || a.x - b.x);
  }

  startPosition(): [number, number, number] | null {
    let best: { x: number; top: number; z: number } | null = null;
    for (const platform of this.mains.values()) {
      if (!best || Math.abs(platform.x) + Math.abs(platform.z) < Math.abs(best.x) + Math.abs(best.z)) best = platform;
    }
    return best ? [best.x, best.top + 90, best.z] : null;
  }

  debug(): { enabled: boolean; islands: number; props: number } {
    let count = 0;
    let props = 0;
    for (const list of this.islands.values()) {
      count += list.length;
      for (const island of list) props += island.props.length;
    }
    return { enabled: this.enabled, islands: count, props };
  }

  async prepare(): Promise<void> {
    if (typeof window === 'undefined') return;
    if (this.preparing || (this.treeTemplate && this.coinTemplate)) return;
    this.preparing = true;
    try {
      const [tree, coin] = await Promise.all([loadSm64Asset(SM64_MODELS.tree), loadSm64Asset(SM64_MODELS.coin)]);
      this.treeTemplate = tree;
      this.coinTemplate = coin;
      if (this.enabled) {
        this.clear();
        this.lastCell = '';
        this.stream(this.lastPosition);
      }
    } catch {
      // Asset loading is optional in headless/test environments.
    } finally {
      this.preparing = false;
    }
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
        this.mains.delete(key);
      }
    }
  }

  private createCell(key: string, x: number, z: number): void {
    const random = this.makeRng(this.seed + x * 73856093 + z * 19349663);
    const list: Island[] = [];
    const biomeIndex = Math.floor((Math.abs(x) + Math.abs(z)) / 4) % this.biomes.length;
    const width = 380 - biomeIndex * 20;
    const depth = 380 - biomeIndex * 20;
    const height = 36 + biomeIndex * 6;
    // Main islands form a reachable spine through every cell: 620-unit centers
    // leave a 240-unit edge gap, and the height wave changes gently.
    const py = 190 + Math.sin(x * 0.65) * 85 + Math.cos(z * 0.52) * 65;
    const px = x * CELL;
    const pz = z * CELL;
    const biome = this.biomes[biomeIndex];
    list.push(this.createIsland(px, py, pz, width, height, depth, random, biome[Math.floor(random() * biome.length)]));
    const distance = Math.hypot(x, z);
    const difficulty = Math.min(1, distance / 12);
    this.mains.set(key, { x: px, y: py, z: pz, top: py + height / 2, width, depth, biome: biomeIndex, difficulty });
    if (this.treeTemplate) {
      const main = list[0];
      for (let i = 0; i < 2; i++) {
        const tree = this.makeTree(px + (random() - 0.5) * width * 0.65, py + height / 2, pz + (random() - 0.5) * depth * 0.65, random);
        main.props.push(tree);
        this.scene.add(tree);
      }
      if (this.coinTemplate) {
        const coin = this.makeCoin(px + (random() - 0.5) * width * 0.5, py + height / 2 + 90, pz + (random() - 0.5) * depth * 0.5);
        main.props.push(coin);
        this.scene.add(coin);
      }
    }

    if (biomeIndex === 1) {
      // Blue biome grows stacked towers that can be jumped from the main island.
      let top = py + height / 2;
      for (let level = 0; level < 3; level++) {
        const towerSize = 150 - level * 24;
        const towerHeight = 26;
        list.push(this.createIsland(
          px + 90 + level * 24, top + towerHeight / 2, pz - 70,
          towerSize, towerHeight, towerSize, random, biome[Math.floor(random() * biome.length)],
        ));
        top += towerHeight;
      }
    } else if (biomeIndex === 2) {
      // Violet biome adds a narrow bridge jutting out over the void.
      list.push(this.createIsland(
        px + 250, py + height / 2 + 6, pz,
        420, 22, 96, random, biome[Math.floor(random() * biome.length)],
      ));
    }

    // Difficulty raises satellite density and tower height without changing the
    // main-island spine (whose gap/height bounds are covered by the reachability test).
    const satelliteChance = 0.45 - difficulty * 0.2;
    if (random() > satelliteChance) {
      const satelliteWidth = 180 + random() * 150;
      const satelliteDepth = 180 + random() * 150;
      list.push(this.createIsland(
        px + 230, py + 70 + random() * 90, pz + (random() - 0.5) * 260,
        satelliteWidth, 30, satelliteDepth, random, biome[Math.floor(random() * biome.length)],
      ));
      if (difficulty > 0.5 && random() > 0.45) {
        list.push(this.createIsland(
          px - 260, py + 120 + random() * 120, pz + (random() - 0.5) * 320,
          150 + random() * 120, 28, 150 + random() * 120, random, biome[Math.floor(random() * biome.length)],
        ));
      }
    }
    this.islands.set(key, list);
  }

  private createIsland(x: number, y: number, z: number, width: number, height: number, depth: number, random: () => number, color: number): Island {
    const mesh = new THREE.Mesh(this.geometry, new THREE.MeshStandardMaterial({
      color,
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
    const props: THREE.Object3D[] = [];
    const add = (a: [number, number, number], b: [number, number, number], c: [number, number, number], d: [number, number, number]) => {
      surfaces.push(...this.world.addQuad(a, b, c, d));
    };
    add([x - hx, top, z - hz], [x - hx, top, z + hz], [x + hx, top, z + hz], [x + hx, top, z - hz]);
    add([x - hx, bottom, z - hz], [x + hx, bottom, z - hz], [x + hx, bottom, z + hz], [x - hx, bottom, z + hz]);
    add([x - hx, top, z - hz], [x - hx, top, z + hz], [x - hx, bottom, z + hz], [x - hx, bottom, z - hz]);
    add([x + hx, top, z - hz], [x + hx, top, z + hz], [x + hx, bottom, z + hz], [x + hx, bottom, z - hz]);
    add([x + hx, top, z + hz], [x - hx, top, z + hz], [x - hx, bottom, z + hz], [x + hx, bottom, z + hz]);
    add([x - hx, top, z - hz], [x + hx, top, z - hz], [x + hx, bottom, z - hz], [x - hx, bottom, z - hz]);
    for (const surface of surfaces) this.allSurfaces.add(surface);
    return { mesh, surfaces, props };
  }

  private makeTree(x: number, top: number, z: number, random: () => number): THREE.Object3D {
    const tree = instantiateSm64(this.treeTemplate!);
    const size = new THREE.Box3().setFromObject(tree).getSize(new THREE.Vector3());
    const height = 100 + random() * 70;
    tree.scale.multiplyScalar(height / Math.max(1, size.y));
    tree.position.set(x, top, z);
    return tree;
  }

  private makeCoin(x: number, y: number, z: number): THREE.Object3D {
    const coin = instantiateSm64(this.coinTemplate!);
    coin.scale.multiplyScalar(0.7);
    coin.position.set(x, y, z);
    coin.userData.objectBillboard = true;
    return coin;
  }

  private removeIsland(island: Island): void {
    for (const surface of island.surfaces) this.allSurfaces.delete(surface);
    this.world.removeSurfaces(new Set(island.surfaces));
    this.scene.remove(island.mesh);
    for (const prop of island.props) this.scene.remove(prop);
    (island.mesh.material as THREE.Material).dispose();
  }

  private clear(): void {
    for (const list of this.islands.values()) for (const island of list) this.removeIsland(island);
    this.islands.clear();
    this.mains.clear();
    this.allSurfaces.clear();
  }

  private makeRng(seed: number): () => number {
    let state = seed >>> 0;
    return () => {
      state = (state * 1664525 + 1013904223) >>> 0;
      return state / 0xffffffff;
    };
  }
}
