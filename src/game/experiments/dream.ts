import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { CollisionWorld, type Surface } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { ACT_FLAG_AIR } from '../../sim/state';
import type { CourseId } from '../courses';
import { courseMusic } from '../music';
import { instantiateSm64, loadSm64Asset, SM64_MODELS } from '../sm64Assets';
import { BIOMES, DreamGenerator, type Decor, type Piece, type Platform, type Surface as DreamSurface } from './dreamGen';
import { addBoxCollision, addFace, sm64Texture, tiledBox, type TextureName } from './geometry';
import type { Experiment, ExperimentHost } from './types';
import { dock, readout, titleCard } from './ui';

/** Far from every course object, so nothing in the hidden course can reach Mario. */
const ORIGIN = { x: 60000, y: 1000, z: 0, heading: Math.PI / 2 };
const AHEAD = 6;
const BEHIND = 2;
const BEST_KEY = 'jumpman.dream.best';

const SKIES = [
  { top: 0x3f8fd6, mid: 0x9ed4f5, bottom: 0xe8f6ff, fog: 0xcfe9fb },
  { top: 0x2b4f9e, mid: 0xf6b26b, bottom: 0xffe3b0, fog: 0xf7cf9d },
  { top: 0x2a1450, mid: 0xc77dff, bottom: 0xffc2e2, fog: 0xe4b7f0 },
];
const MUSIC: CourseId[] = ['bob', 'rr' as CourseId, 'ddd' as CourseId];

const SURFACE_TEXTURE: Record<DreamSurface, TextureName> = {
  grass: 'grassTop', stone: 'flagstone', brick: 'brick', wood: 'planks', checker: 'checker', gold: 'gold',
  tiles: 'tiles', cloud: 'whiteBrick', whiteBrick: 'whiteBrick', rainbow: 'rainbow',
};

interface Built {
  piece: Piece;
  group: THREE.Group;
  surfaces: Set<Surface>;
  coins: Array<{ group: THREE.Object3D; pos: Vec3; taken: boolean }>;
  spinners: THREE.Object3D[];
  flags: Array<{ platform: Platform; flag: THREE.Mesh; reached: boolean }>;
}

/**
 * Mario's Infinite Dream: an endless, seeded chain of floating set pieces
 * streamed around Mario, with its own collision world and sky.
 */
export class InfiniteDreamExperiment implements Experiment {
  readonly id = 'dream';
  readonly title = 'Infinite Dream';
  readonly course = null;
  readonly replacesCourse = true;
  private host!: ExperimentHost;
  private readonly world = new CollisionWorld(1024, 1e9);
  private generator!: DreamGenerator;
  private readonly built = new Map<number, Built>();
  private readonly surfaceOwner = new Map<Surface, { piece: number; platform: Platform }>();
  private readonly root = new THREE.Group();
  private readonly materials = new Map<string, THREE.Material>();
  private sky!: THREE.Mesh;
  private skyUniforms!: { topColor: { value: THREE.Color }; midColor: { value: THREE.Color }; bottomColor: { value: THREE.Color } };
  private saved: { fog: THREE.Scene['fog']; background: THREE.Scene['background']; far: number } | null = null;
  private tree: GLTF | null = null;
  private coin: GLTF | null = null;
  private current = 0;
  private safe: Vec3 = [ORIGIN.x, ORIGIN.y, ORIGIN.z];
  private biome = -1;
  private coins = 0;
  private wakeUps = 0;
  private best = 0;
  private distance = 0;
  private readonly seed: number;
  private readonly skyColor = { top: new THREE.Color(), mid: new THREE.Color(), bottom: new THREE.Color(), fog: new THREE.Color() };
  private readouts: Array<(value: string) => void> = [];

  constructor(seed?: number) {
    const param = typeof location !== 'undefined' ? Number(new URLSearchParams(location.search).get('dreamSeed')) : NaN;
    this.seed = seed ?? (Number.isFinite(param) && param > 0 ? param : 1 + Math.floor(Math.random() * 99999));
  }

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    this.generator = new DreamGenerator(this.seed, ORIGIN);
    try { this.best = Number(localStorage.getItem(BEST_KEY) ?? 0) || 0; } catch { /* storage optional */ }
    this.root.name = 'infinite-dream';
    host.scene.add(this.root);
    this.saved = { fog: host.scene.fog, background: host.scene.background, far: host.camera.far };
    this.buildSky();
    host.camera.far = 70000;
    host.camera.updateProjectionMatrix();
    titleCard(host.ui, '03', 'Infinite Dream', 'An endless seeded sky of floating set pieces. Every gap is within one jump. It gets stranger.', [
      ['WASD · Space', 'run · jump'], ['Fall', 'wake up at the last checkpoint'], ['Seed', String(this.seed)],
    ]);
    const bar = dock(host.ui);
    bar.classList.add('xp-dock-stats');
    this.readouts = [readout(bar, 'Distance'), readout(bar, 'Best'), readout(bar, 'Biome'), readout(bar, 'Coins')];
    try {
      [this.tree, this.coin] = await Promise.all([loadSm64Asset(SM64_MODELS.tree), loadSm64Asset(SM64_MODELS.coin)]);
    } catch { /* decor is optional */ }
    for (let i = 0; i <= AHEAD; i++) this.build(i);
    const start = this.generator.piece(0).platforms[0];
    this.safe = [start.x, start.y, start.z];
    host.setMarioWorld(this.world, { pos: [start.x, start.y + 400, start.z], faceYaw: Math.round(ORIGIN.heading * 32768 / Math.PI) }, -1e9, []);
    host.placeMario([start.x, start.y + 600, start.z], Math.round(ORIGIN.heading * 32768 / Math.PI));
    host.toast(`INFINITE DREAM · SEED ${this.seed}`, 3000);
  }

  exit(): void {
    for (const index of [...this.built.keys()]) this.retire(index);
    this.root.removeFromParent();
    this.sky.removeFromParent();
    this.sky.geometry.dispose();
    (this.sky.material as THREE.Material).dispose();
    for (const material of this.materials.values()) material.dispose();
    this.materials.clear();
    const { host } = this;
    if (this.saved) {
      host.scene.fog = this.saved.fog;
      host.scene.background = this.saved.background;
      host.camera.far = this.saved.far;
      host.camera.updateProjectionMatrix();
    }
    host.restoreMarioWorld();
  }

  // ------------------------------------------------------------- building

  private material(surface: DreamSurface, tint = 0xffffff, side = false): THREE.Material {
    const key = `${surface}:${tint}:${side}`;
    let material = this.materials.get(key);
    if (!material) {
      const texture = side ? (surface === 'grass' ? 'grassEdge' : SURFACE_TEXTURE[surface]) : SURFACE_TEXTURE[surface];
      material = new THREE.MeshStandardMaterial({ map: sm64Texture(texture), color: tint, roughness: 0.85, metalness: surface === 'gold' ? 0.25 : 0 });
      this.materials.set(key, material);
    }
    return material;
  }

  private plain(color: number, emissive = 0): THREE.Material {
    const key = `plain:${color}:${emissive}`;
    let material = this.materials.get(key);
    if (!material) {
      material = new THREE.MeshStandardMaterial({ color, emissive, roughness: 0.7 });
      this.materials.set(key, material);
    }
    return material;
  }

  private build(index: number): void {
    if (this.built.has(index) || index < 0) return;
    const piece = this.generator.piece(index);
    const built: Built = { piece, group: new THREE.Group(), surfaces: new Set(), coins: [], spinners: [], flags: [] };
    for (const platform of piece.platforms) this.buildPlatform(built, platform);
    for (const decor of piece.decor) this.buildDecor(built, decor);
    for (const spot of piece.coins) {
      if (!this.coin) break;
      const group = instantiateSm64(this.coin);
      group.scale.multiplyScalar(0.85);
      group.userData.objectBillboard = true;
      group.position.set(spot.x, spot.y, spot.z);
      built.group.add(group);
      built.coins.push({ group, pos: [spot.x, spot.y, spot.z], taken: false });
    }
    // An invisible catch floor far below keeps Mario's floor query valid while he falls.
    const b = piece.bounds;
    const y = b.minY - 3200;
    const pad = 3500;
    addFace(this.world, [[b.minX - pad, y, b.minZ - pad], [b.maxX + pad, y, b.minZ - pad], [b.maxX + pad, y, b.maxZ + pad], [b.minX - pad, y, b.maxZ + pad]], [0, 1, 0], built.surfaces, { type: 0 });
    this.root.add(built.group);
    this.built.set(index, built);
  }

  private retire(index: number): void {
    const built = this.built.get(index);
    if (!built) return;
    this.world.removeSurfaces(built.surfaces);
    for (const surface of built.surfaces) this.surfaceOwner.delete(surface);
    built.group.traverse(node => {
      if (node instanceof THREE.Mesh && !node.userData.sharedSm64Asset) node.geometry.dispose();
    });
    built.group.removeFromParent();
    this.built.delete(index);
  }

  private own(built: Built, platform: Platform, before: number): void {
    const surfaces = [...built.surfaces].slice(before);
    for (const surface of surfaces) this.surfaceOwner.set(surface, { piece: built.piece.index, platform });
  }

  private buildPlatform(built: Built, p: Platform): void {
    const before = built.surfaces.size;
    const tint = p.tint ?? 0xffffff;
    if (p.shape === 'island' || p.shape === 'pillar') {
      const radius = p.w / 2;
      const segments = p.shape === 'island' ? 14 : 16;
      const rim = p.shape === 'island' ? Math.min(p.h, 70) : p.h;
      const top = new THREE.CylinderGeometry(radius, radius * (p.shape === 'island' ? 0.94 : 1.04), rim, segments, 1);
      scaleCylinderUv(top, radius, rim);
      const materials = [this.material(p.surface, tint, true), this.material(p.surface, tint), this.material(p.surface, tint, true)];
      const mesh = new THREE.Mesh(top, materials);
      mesh.position.set(p.x, p.y - rim / 2, p.z);
      mesh.castShadow = mesh.receiveShadow = true;
      built.group.add(mesh);
      if (p.shape === 'island') {
        const under = islandUnderside(radius * 0.94, radius * (0.9 + (Math.abs(p.x * 13 + p.z * 7) % 40) / 100), segments, p.x + p.z);
        const rock = new THREE.Mesh(under, this.material(p.surface === 'grass' ? 'grass' : p.surface, tint === 0xffffff ? 0xc9a37a : tint, true));
        rock.position.set(p.x, p.y - rim, p.z);
        rock.castShadow = true;
        built.group.add(rock);
      }
      // Collision: the top polygon plus walls around the rim.
      const ring: Vec3[] = [];
      for (let i = 0; i < segments; i++) {
        const a = (i / segments) * Math.PI * 2;
        ring.push([p.x + Math.sin(a) * radius, p.y, p.z + Math.cos(a) * radius]);
      }
      addFace(this.world, ring, [0, 1, 0], built.surfaces);
      const wallDepth = Math.min(p.h, 3000);
      for (let i = 0; i < segments; i++) {
        const a = ring[i], b = ring[(i + 1) % segments];
        const mid = (i + 0.5) / segments * Math.PI * 2;
        addFace(this.world, [a, b, [b[0], p.y - wallDepth, b[2]], [a[0], p.y - wallDepth, a[2]]], [Math.sin(mid), 0, Math.cos(mid)], built.surfaces);
      }
    } else {
      const size = new THREE.Vector3(p.w, p.h, p.d);
      const center = new THREE.Vector3(p.x, p.y - p.h / 2, p.z);
      const geometry = tiledBox(p.w, p.h, p.d, p.shape === 'plank' ? 120 : 220);
      const mesh = new THREE.Mesh(geometry, this.material(p.surface, tint));
      mesh.position.copy(center);
      mesh.rotation.y = p.yaw;
      mesh.castShadow = mesh.receiveShadow = true;
      built.group.add(mesh);
      addBoxCollision(this.world, center, size, p.yaw, built.surfaces);
    }
    this.own(built, p, before);
    if (p.checkpoint) {
      const pole = new THREE.Mesh(new THREE.CylinderGeometry(8, 8, 420, 8), this.plain(0xeeeeee));
      const offset = p.w * 0.28;
      pole.position.set(p.x + offset, p.y + 210, p.z);
      const flag = new THREE.Mesh(new THREE.BufferGeometry().setFromPoints([
        new THREE.Vector3(0, 0, 0), new THREE.Vector3(0, -110, 0), new THREE.Vector3(160, -55, 0),
      ]), new THREE.MeshStandardMaterial({ color: 0xe54335, side: THREE.DoubleSide }));
      flag.geometry.computeVertexNormals();
      flag.position.set(p.x + offset, p.y + 410, p.z);
      built.group.add(pole, flag);
      built.flags.push({ platform: p, flag, reached: false });
    }
  }

  private buildDecor(built: Built, d: Decor): void {
    const group = built.group;
    switch (d.kind) {
      case 'tree': {
        if (!this.tree) return;
        const tree = instantiateSm64(this.tree);
        fitHeight(tree, 320 * d.scale);
        tree.position.set(d.x, d.y, d.z);
        tree.rotation.y = d.yaw;
        group.add(tree);
        return;
      }
      case 'post': return;
      case 'arch': case 'door': {
        const frame = new THREE.Group();
        const color = d.kind === 'door' ? (d.tint ?? 0xffb3d9) : 0xffffff;
        const surface: DreamSurface = d.kind === 'door' ? 'whiteBrick' : 'brick';
        for (const side of [-1, 1]) {
          const post = new THREE.Mesh(tiledBox(70, 460, 70), this.material(surface, color));
          post.position.set(side * 190, 230, 0);
          frame.add(post);
        }
        const lintel = new THREE.Mesh(tiledBox(450, 70, 80), this.material(surface, color));
        lintel.position.y = 490;
        frame.add(lintel);
        if (d.kind === 'door') {
          const portal = new THREE.Mesh(new THREE.PlaneGeometry(310, 450), this.plain(0x1b0f3a, 0x3a1f7a));
          (portal.material as THREE.MeshStandardMaterial).side = THREE.DoubleSide;
          portal.position.y = 228;
          frame.add(portal);
        }
        frame.position.set(d.x, d.y, d.z);
        frame.rotation.y = d.yaw;
        frame.scale.setScalar(d.scale);
        group.add(frame);
        return;
      }
      case 'tower': {
        const tower = new THREE.Group();
        const height = 1400 * d.scale;
        const body = new THREE.Mesh(new THREE.CylinderGeometry(260, 300, height, 16), this.material('brick', 0xffffff, true));
        scaleCylinderUv(body.geometry as THREE.CylinderGeometry, 280, height);
        body.position.y = height / 2;
        const roof = new THREE.Mesh(new THREE.ConeGeometry(340, 420, 16), this.plain(0xc0392b));
        roof.position.y = height + 210;
        tower.add(body, roof);
        tower.position.set(d.x, d.y - height * 0.6, d.z);
        group.add(tower);
        return;
      }
      case 'ruin': {
        for (let i = 0; i < 3; i++) {
          const column = new THREE.Mesh(new THREE.CylinderGeometry(70, 80, 400 + i * 160, 10), this.material('stone', 0xffffff, true));
          column.position.set(d.x + Math.sin(d.yaw + i * 2.1) * 260, d.y + 200 + i * 80, d.z + Math.cos(d.yaw + i * 2.1) * 260);
          column.rotation.z = (i - 1) * 0.18;
          group.add(column);
        }
        const slab = new THREE.Mesh(tiledBox(700, 120, 700), this.material('stone'));
        slab.position.set(d.x, d.y - 60, d.z);
        group.add(slab);
        return;
      }
      case 'inverted': case 'farIsland': {
        const radius = 380 * d.scale;
        const island = new THREE.Group();
        const top = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius * 0.94, 60, 12), [this.material('grass', d.kind === 'inverted' ? (d.tint ?? 0xffffff) : 0xffffff, true), this.material('grass', d.kind === 'inverted' ? (d.tint ?? 0xffffff) : 0xffffff), this.material('grass', 0xffffff, true)]);
        const under = new THREE.Mesh(islandUnderside(radius * 0.94, radius, 12, d.x), this.material('grass', 0xc9a37a, true));
        under.position.y = -30;
        island.add(top, under);
        if (this.tree) {
          const tree = instantiateSm64(this.tree);
          fitHeight(tree, radius * 0.8);
          tree.position.y = 30;
          island.add(tree);
        }
        island.position.set(d.x, d.y, d.z);
        if (d.kind === 'inverted') {
          island.rotation.x = Math.PI;
          built.spinners.push(island);
          island.userData.spin = 0.05;
        }
        group.add(island);
        return;
      }
      case 'cube': {
        const size = 500 * d.scale;
        const cube = new THREE.Mesh(tiledBox(size, size, size, 250), this.material('checker', d.tint ?? 0xffffff));
        cube.position.set(d.x, d.y, d.z);
        cube.rotation.set(d.yaw, d.yaw * 0.7, 0.4);
        cube.userData.spin = 0.15;
        built.spinners.push(cube);
        group.add(cube);
        return;
      }
      case 'ring': {
        const ring = new THREE.Mesh(new THREE.TorusGeometry(260, 34, 10, 36), new THREE.MeshStandardMaterial({ color: 0xffd24a, metalness: 0.6, roughness: 0.25, emissive: 0x332200 }));
        ring.position.set(d.x, d.y, d.z);
        ring.userData.spin = 0.8;
        built.spinners.push(ring);
        group.add(ring);
        return;
      }
      default:
    }
  }

  private buildSky(): void {
    const uniforms = {
      topColor: { value: new THREE.Color(SKIES[0].top) },
      midColor: { value: new THREE.Color(SKIES[0].mid) },
      bottomColor: { value: new THREE.Color(SKIES[0].bottom) },
    };
    this.skyUniforms = uniforms;
    this.sky = new THREE.Mesh(new THREE.SphereGeometry(60000, 32, 16), new THREE.ShaderMaterial({
      side: THREE.BackSide, depthWrite: false, fog: false, uniforms,
      vertexShader: `varying vec3 vDir; void main() { vDir = normalize(position); gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
      fragmentShader: `uniform vec3 topColor; uniform vec3 midColor; uniform vec3 bottomColor; varying vec3 vDir;
        void main() { float h = vDir.y; vec3 c = mix(bottomColor, midColor, smoothstep(-0.35, 0.08, h)); c = mix(c, topColor, smoothstep(0.08, 0.7, h)); gl_FragColor = vec4(c, 1.0); }`,
    }));
    this.sky.renderOrder = -10;
    this.sky.frustumCulled = false;
    this.host.scene.add(this.sky);
    this.host.scene.background = null;
    this.host.scene.fog = new THREE.Fog(SKIES[0].fog, 9000, 42000);
  }

  // ------------------------------------------------------------- per tick

  afterStep(): void {
    const m = this.host.mario;
    const owner = m.floor ? this.surfaceOwner.get(m.floor) : undefined;
    const grounded = !(m.action & ACT_FLAG_AIR) && m.pos[1] <= m.floorHeight + 4;
    if (owner && grounded) {
      this.current = owner.piece;
      if (owner.platform.route) {
        this.safe = [owner.platform.x, owner.platform.y, owner.platform.z];
        m.spawn = { pos: [...this.safe], faceYaw: m.faceAngle[1] };
      }
      for (const flag of this.built.get(owner.piece)?.flags ?? []) {
        if (flag.platform !== owner.platform || flag.reached) continue;
        flag.reached = true;
        (flag.flag.material as THREE.MeshStandardMaterial).color.setHex(0xffd24a);
        this.host.sfx.starSound();
        this.host.particles.sparks([flag.platform.x, flag.platform.y + 300, flag.platform.z], 20);
        this.host.toast(`CHECKPOINT · ${BIOMES[this.generator.piece(owner.piece).biome].name.toUpperCase()}`, 1800);
      }
    }
    // Falling past the route wakes Mario at the last platform he stood on.
    if (m.pos[1] < this.safe[1] - 2200) {
      this.wakeUps++;
      this.host.placeMario([this.safe[0], this.safe[1] + 500, this.safe[2]], m.faceAngle[1]);
      this.host.sfx.teleport();
      this.host.toast('…AND YOU WAKE UP', 1400);
    }
    // Stream: build ahead, retire behind (one build per tick avoids hitches).
    for (let i = this.current - BEHIND; i <= this.current + AHEAD; i++) {
      if (!this.built.has(i) && i >= 0) { this.build(i); break; }
    }
    for (const index of [...this.built.keys()]) if (index < this.current - BEHIND - 1 || index > this.current + AHEAD + 2) this.retire(index);
    // Coins.
    for (const built of this.built.values()) {
      for (const coin of built.coins) {
        if (coin.taken) continue;
        if (Math.hypot(coin.pos[0] - m.pos[0], coin.pos[1] - (m.pos[1] + 70), coin.pos[2] - m.pos[2]) < 120) {
          coin.taken = true;
          coin.group.visible = false;
          this.coins++;
          this.host.addCoins(1);
          this.host.sfx.coin();
          this.host.particles.sparks(coin.pos, 6);
        }
      }
    }
    const biome = this.generator.piece(Math.max(0, this.current)).biome;
    if (biome !== this.biome) {
      if (this.biome >= 0) this.host.toast(BIOMES[biome].name.toUpperCase(), 2600);
      this.biome = biome;
      this.host.sfx.startMusic(courseMusic(MUSIC[biome]));
    }
    this.distance = Math.max(this.distance, (m.pos[0] - ORIGIN.x) / 100);
    if (this.distance > this.best) {
      this.best = this.distance;
      if (Math.floor(this.best) % 10 === 0) try { localStorage.setItem(BEST_KEY, String(Math.floor(this.best))); } catch { /* optional */ }
    }
  }

  frame(dt: number): void {
    const { camera, renderPos } = this.host;
    this.sky.position.copy(camera.position);
    // Blend the sky toward the biome under Mario's feet.
    const target = SKIES[Math.max(0, this.biome)];
    const k = Math.min(1, dt * 0.6);
    this.skyColor.top.lerp(new THREE.Color(target.top), k);
    this.skyColor.mid.lerp(new THREE.Color(target.mid), k);
    this.skyColor.bottom.lerp(new THREE.Color(target.bottom), k);
    this.skyColor.fog.lerp(new THREE.Color(target.fog), k);
    this.skyUniforms.topColor.value.copy(this.skyColor.top);
    this.skyUniforms.midColor.value.copy(this.skyColor.mid);
    this.skyUniforms.bottomColor.value.copy(this.skyColor.bottom);
    if (this.host.scene.fog instanceof THREE.Fog) this.host.scene.fog.color.copy(this.skyColor.fog);
    for (const built of this.built.values()) {
      for (const spinner of built.spinners) spinner.rotation.y += dt * (spinner.userData.spin ?? 0.2);
      for (const coin of built.coins) if (!coin.taken) coin.group.rotation.y += dt * 3;
    }
    this.readouts[0]?.(`${Math.floor(Math.max(0, (renderPos.x - ORIGIN.x) / 100)).toLocaleString()} m`);
    this.readouts[1]?.(`${Math.floor(this.best).toLocaleString()} m`);
    this.readouts[2]?.(BIOMES[Math.max(0, this.biome)].name);
    this.readouts[3]?.(String(this.coins));
  }

  debug(): Record<string, unknown> {
    return {
      seed: this.seed,
      current: this.current,
      built: [...this.built.keys()].sort((a, b) => a - b),
      biome: this.biome,
      biomeName: BIOMES[Math.max(0, this.biome)].name,
      coins: this.coins,
      wakeUps: this.wakeUps,
      distance: this.distance,
      surfaces: this.world.surfaces.length,
      safe: this.safe,
      kinds: [...this.built.values()].map(built => built.piece.kind),
      route: [...this.built.values()].flatMap(built => built.piece.platforms.filter(p => p.route).map(p => ({ piece: built.piece.index, x: p.x, y: p.y, z: p.z, shape: p.shape }))),
    };
  }
}

function fitHeight(object: THREE.Object3D, height: number): void {
  const size = new THREE.Box3().setFromObject(object).getSize(new THREE.Vector3());
  object.scale.multiplyScalar(height / Math.max(1, size.y));
}

/** World-scale UVs for a cylinder's side and caps. */
function scaleCylinderUv(geometry: THREE.CylinderGeometry, radius: number, height: number): void {
  const uv = geometry.getAttribute('uv') as THREE.BufferAttribute;
  const groups = geometry.groups;
  const side = groups[0];
  const circumference = Math.PI * 2 * radius;
  const index = geometry.getIndex()!;
  const touched = new Set<number>();
  for (let i = side.start; i < side.start + side.count; i++) touched.add(index.getX(i));
  for (const vertex of touched) uv.setXY(vertex, uv.getX(vertex) * circumference / 256, uv.getY(vertex) * Math.max(1, height / 256));
  for (let g = 1; g < groups.length; g++) {
    const caps = new Set<number>();
    for (let i = groups[g].start; i < groups[g].start + groups[g].count; i++) caps.add(index.getX(i));
    for (const vertex of caps) uv.setXY(vertex, (uv.getX(vertex) - 0.5) * radius * 2 / 256, (uv.getY(vertex) - 0.5) * radius * 2 / 256);
  }
  uv.needsUpdate = true;
}

/** A jagged inverted cone of rock and earth hanging under an island. */
function islandUnderside(radius: number, depth: number, segments: number, seed: number): THREE.BufferGeometry {
  const geometry = new THREE.ConeGeometry(radius, depth, segments, 3, true);
  geometry.rotateX(Math.PI);
  geometry.translate(0, -depth / 2, 0);
  const position = geometry.getAttribute('position') as THREE.BufferAttribute;
  let state = Math.abs(Math.floor(seed)) % 2147483647 || 7;
  const random = () => (state = (state * 16807) % 2147483647) / 2147483647;
  for (let i = 0; i < position.count; i++) {
    const y = position.getY(i);
    if (y > -1) continue;
    const t = -y / depth;
    const wobble = 1 + (random() - 0.5) * 0.35 * (1 - t);
    position.setXYZ(i, position.getX(i) * wobble, y * (1 + (random() - 0.5) * 0.15), position.getZ(i) * wobble);
  }
  geometry.computeVertexNormals();
  const uv = geometry.getAttribute('uv') as THREE.BufferAttribute;
  for (let i = 0; i < uv.count; i++) uv.setXY(i, uv.getX(i) * radius * 6 / 256, uv.getY(i) * depth / 256);
  return geometry;
}
