import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { CollisionWorld, type Surface } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { ACT_FLAG_AIR, ACT_JUMP, emptyController, type ControllerSnapshot } from '../../sim/state';
import type { CourseId } from '../courses';
import { courseMusic } from '../music';
import { instantiateSm64, loadSm64Asset, SM64_MODELS } from '../sm64Assets';
import { addBoxCollision, addFace, sm64Texture, tiledBox, type TextureName } from './geometry';
import { GhostMario, recordFrame, type GhostFrame } from './ghost';
import { RunnerTrack, THEMES, type Block, type Hazard, type Segment } from './runnerGen';
import { SkyDome, type SkyColors } from './sky';
import type { Experiment, ExperimentHost, StepOverride } from './types';
import { dock, el, readout, segmented, titleCard, type Segmented } from './ui';

/** Far from every course object, so nothing in the hidden course reaches Mario. */
const ORIGIN_X = 120000;
const ORIGIN_TOP = 1000;
const AHEAD = 9000;
const BEHIND = 2500;
const BEST_KEY = 'jumpman.runner.best';
const GHOST_KEY = 'jumpman.runner.ghost';

/** Today's shared seed: everyone runs the same track on the same day. */
export function dailySeed(date = new Date()): number {
  return date.getFullYear() * 10000 + (date.getMonth() + 1) * 100 + date.getDate();
}
function storage<T>(key: string, value?: T): T | null {
  try {
    if (value === undefined) { const raw = localStorage.getItem(key); return raw ? JSON.parse(raw) as T : null; }
    localStorage.setItem(key, JSON.stringify(value));
  } catch { /* storage is optional (and may be full) */ }
  return value ?? null;
}
const COUNTDOWN = 45;

const SKIES: SkyColors[] = [
  { top: 0x2f7fd6, mid: 0x8fcdf5, bottom: 0xe9f6ff },
  { top: 0x3a3f8f, mid: 0xf29a5c, bottom: 0xffe0b0 },
  { top: 0x3b1d73, mid: 0xd48cff, bottom: 0xffd0ec },
];
const FOG = [0xcfe9fb, 0xf7cf9d, 0xeac0f2];
const MUSIC: CourseId[] = ['rr' as CourseId, 'bitdw' as CourseId, 'rr' as CourseId];
const PASTELS = [0xffb3d9, 0xb3e5ff, 0xd4b3ff, 0xfff0a3, 0xb3ffd1];

const LOOK: Record<number, Record<Block['kind'], { top: TextureName; side: TextureName }>> = {
  0: { floor: { top: 'grassTop', side: 'grassEdge' }, bridge: { top: 'planks', side: 'planks' }, hurdle: { top: 'planks', side: 'planks' }, pillar: { top: 'stone', side: 'stone' } },
  1: { floor: { top: 'flagstone', side: 'brick' }, bridge: { top: 'planks', side: 'planks' }, hurdle: { top: 'brick', side: 'brick' }, pillar: { top: 'sandstone', side: 'sandstone' } },
  2: { floor: { top: 'checker', side: 'whiteBrick' }, bridge: { top: 'rainbow', side: 'rainbow' }, hurdle: { top: 'whiteBrick', side: 'whiteBrick' }, pillar: { top: 'gold', side: 'gold' } },
};

interface Built {
  segment: Segment;
  group: THREE.Group;
  surfaces: Set<Surface>;
  coins: Array<{ group: THREE.Object3D; pos: Vec3; taken: boolean }>;
  spinners: THREE.Object3D[];
  hazards: Array<{ hazard: Hazard; mesh: THREE.Mesh; z: number }>;
}

type RunState = 'ready' | 'running' | 'over';

/**
 * Endless Run: Mario auto-runs down a generated track using his real SM64
 * physics. You steer, jump, long-jump and dive; one fall ends the run.
 */
export class EndlessRunnerExperiment implements Experiment {
  readonly id = 'runner';
  readonly title = 'Endless Run';
  readonly course = null;
  readonly replacesCourse = true;
  private host!: ExperimentHost;
  private world = new CollisionWorld(1024, 1e9);
  private track: RunnerTrack | null = null;
  private seed: number;
  private mode: 'daily' | 'random' = 'daily';
  private modeSeg!: Segmented<'daily' | 'random'>;
  /** Pillar collision faces, for glancing off instead of stopping dead. */
  private readonly pillarFaces = new Map<Surface, Block>();
  private recording: GhostFrame[] = [];
  private ghostFrames: GhostFrame[] = [];
  private ghost: GhostMario | null = null;
  private runTicks = 0;
  private hits = 0;
  private blocked = 0;
  private readonly built = new Map<number, Built>();
  private readonly root = new THREE.Group();
  private readonly materials = new Map<string, THREE.Material>();
  private sky!: SkyDome;
  private saved: { fog: THREE.Scene['fog']; background: THREE.Scene['background']; far: number; fov: number } | null = null;
  private coinModel: GLTF | null = null;
  private treeModel: GLTF | null = null;
  private state: RunState = 'ready';
  private ticks = 0;
  private coins = 0;
  private best = 0;
  private distance = 0;
  private theme = -1;
  private runs = 0;
  private lastTop = ORIGIN_TOP;
  private readonly camPos = new THREE.Vector3();
  private readonly camLook = new THREE.Vector3();
  private readouts: Array<(value: string) => void> = [];
  private overCard!: HTMLElement;

  constructor(seed?: number) {
    const param = typeof location !== 'undefined' ? Number(new URLSearchParams(location.search).get('runSeed')) : NaN;
    const fixed = seed ?? (Number.isFinite(param) && param > 0 ? param : null);
    if (fixed !== null) this.mode = 'random';
    this.seed = fixed ?? dailySeed();
  }

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    this.root.name = 'endless-run';
    host.scene.add(this.root);
    this.saved = { fog: host.scene.fog, background: host.scene.background, far: host.camera.far, fov: host.camera.fov };
    this.sky = new SkyDome(SKIES[0], 60000, 0.6);
    host.scene.add(this.sky.mesh);
    host.scene.background = null;
    host.scene.fog = new THREE.Fog(FOG[0], 7000, 26000);
    host.camera.far = 80000;
    host.camera.fov = 62;
    host.camera.updateProjectionMatrix();
    titleCard(host.ui, '03', 'Endless Run', 'Mario never stops running. Steer, jump the gaps and hurdles, dodge the pillars. One fall ends the run.', [
      ['A / D', 'steer'], ['Space', 'jump (hold for height)'], ['Ctrl/K + Space', 'long jump'], ['Space after a fall', 'run again'],
    ]);
    const bar = dock(host.ui);
    bar.classList.add('xp-dock-stats');
    this.modeSeg = segmented([
      { value: 'daily', label: 'Daily', title: 'Today’s track — the same for everyone, with your ghost' },
      { value: 'random', label: 'Random', title: 'A new track every run' },
    ], this.mode, mode => {
      this.mode = mode;
      this.modeSeg.set(mode);
      this.seed = mode === 'daily' ? dailySeed() : 1 + Math.floor(Math.random() * 99999);
      this.startRun();
    });
    bar.append(this.modeSeg.element);
    this.readouts = [readout(bar, 'Distance'), readout(bar, 'Best'), readout(bar, 'Coins'), readout(bar, 'Zone')];
    this.overCard = el('section', 'xp-gameover');
    this.overCard.hidden = true;
    host.ui.append(this.overCard);
    try {
      [this.coinModel, this.treeModel] = await Promise.all([loadSm64Asset(SM64_MODELS.coin), loadSm64Asset(SM64_MODELS.tree)]);
    } catch { /* decor is optional */ }
    try { this.ghost = await GhostMario.create(); this.ghost.root.visible = false; host.scene.add(this.ghost.root); } catch { /* no ghost */ }
    this.startRun();
  }

  private bestKey(): string {
    return this.mode === 'daily' ? `${BEST_KEY}.daily.${this.seed}` : BEST_KEY;
  }

  private startRun(): void {
    for (const index of [...this.built.keys()]) this.retire(index);
    this.pillarFaces.clear();
    this.best = storage<number>(this.bestKey()) ?? 0;
    // Daily runs race the ghost of your best attempt on today's track.
    this.ghostFrames = this.mode === 'daily' ? storage<GhostFrame[]>(`${GHOST_KEY}.${this.seed}`) ?? [] : [];
    this.recording = [];
    this.runTicks = 0;
    this.hits = 0;
    this.world = new CollisionWorld(1024, 1e9);
    this.track = new RunnerTrack(this.seed, ORIGIN_X, ORIGIN_TOP);
    this.stream(ORIGIN_X);
    const faceYaw = 0x4000; // +X
    this.host.setMarioWorld(this.world, { pos: [ORIGIN_X, ORIGIN_TOP + 300, 0], faceYaw }, -1e9, []);
    this.host.placeMario([ORIGIN_X, ORIGIN_TOP, 0], faceYaw);
    this.state = 'ready';
    this.ticks = 0;
    this.coins = 0;
    this.distance = 0;
    this.lastTop = ORIGIN_TOP;
    this.theme = -1;
    this.runs++;
    this.overCard.hidden = true;
    this.camPos.set(0, 0, 0);
    this.host.toast(this.mode === 'daily' ? `DAILY RUN · ${this.seed}${this.ghostFrames.length ? ' · BEAT YOUR GHOST' : ''}` : 'READY…', 1400);
  }

  exit(): void {
    for (const index of [...this.built.keys()]) this.retire(index);
    this.root.removeFromParent();
    this.ghost?.dispose();
    this.ghost = null;
    this.sky.dispose();
    for (const material of this.materials.values()) material.dispose();
    this.materials.clear();
    const { host } = this;
    if (this.saved) {
      host.scene.fog = this.saved.fog;
      host.scene.background = this.saved.background;
      host.camera.far = this.saved.far;
      host.camera.fov = this.saved.fov;
      host.camera.updateProjectionMatrix();
    }
    host.restoreMarioWorld();
  }

  keydown(event: KeyboardEvent): boolean {
    if (this.state === 'over' && (event.code === 'Space' || event.code === 'Enter' || event.code === 'KeyR')) {
      event.preventDefault();
      if (this.mode === 'random') this.seed = 1 + Math.floor(Math.random() * 99999);
      this.startRun();
      return true;
    }
    return false;
  }

  // ------------------------------------------------------------- building

  private material(texture: TextureName, tint = 0xffffff): THREE.Material {
    const key = `${texture}:${tint}`;
    let material = this.materials.get(key);
    if (!material) {
      material = new THREE.MeshStandardMaterial({ map: sm64Texture(texture), color: tint, roughness: 0.85, metalness: texture === 'gold' ? 0.25 : 0 });
      this.materials.set(key, material);
    }
    return material;
  }

  private stream(x: number): void {
    // Build every segment that starts within reach ahead; retire those far behind.
    for (let i = 0; ; i++) {
      const seg = this.track!.segment(i);
      if (seg.start > x + AHEAD) break;
      if (seg.end < x - BEHIND) { this.retire(i); continue; }
      if (!this.built.has(i)) this.build(seg);
    }
  }

  private build(segment: Segment): void {
    const built: Built = { segment, group: new THREE.Group(), surfaces: new Set(), coins: [], spinners: [], hazards: [] };
    let lowest = Infinity;
    segment.blocks.forEach((block, n) => {
      const length = block.x1 - block.x0;
      const center = new THREE.Vector3((block.x0 + block.x1) / 2, block.top - block.height / 2, block.z);
      const size = new THREE.Vector3(length, block.height, block.width);
      const look = LOOK[block.theme][block.kind];
      const tint = block.theme === 2 && block.kind !== 'bridge' ? PASTELS[(segment.index + n) % PASTELS.length] : 0xffffff;
      const side = this.material(look.side, tint === 0xffffff || block.kind !== 'floor' ? tint : 0xffffff);
      const top = this.material(look.top, tint);
      // BoxGeometry groups: +x, -x, +y, -y, +z, -z.
      const mesh = new THREE.Mesh(tiledBox(size.x, size.y, size.z, block.kind === 'bridge' ? 140 : 240), [side, side, top, side, side, side]);
      mesh.position.copy(center);
      mesh.castShadow = mesh.receiveShadow = true;
      built.group.add(mesh);
      const faces = new Set<Surface>();
      addBoxCollision(this.world, center, size, 0, faces);
      for (const face of faces) {
        built.surfaces.add(face);
        if (block.kind === 'pillar' || block.kind === 'hurdle') this.pillarFaces.set(face, block);
      }
      lowest = Math.min(lowest, block.top);
    });
    for (const [x, y, z] of segment.coins) {
      if (!this.coinModel) break;
      const coin = instantiateSm64(this.coinModel);
      coin.scale.multiplyScalar(0.85);
      coin.userData.objectBillboard = true;
      coin.position.set(x, y, z);
      built.group.add(coin);
      built.coins.push({ group: coin, pos: [x, y, z], taken: false });
    }
    for (const hazard of segment.hazards) {
      const mesh = new THREE.Mesh(new THREE.IcosahedronGeometry(hazard.radius, 1), this.material('stone', 0xb0a090));
      mesh.castShadow = true;
      mesh.position.set(hazard.x, hazard.top + hazard.radius, 0);
      built.group.add(mesh);
      built.hazards.push({ hazard, mesh, z: 0 });
    }
    this.decorate(built);
    // A catch floor far below keeps Mario's floor query valid while he falls.
    const y = lowest - 3500;
    addFace(this.world, [[segment.start - 3000, y, -6000], [segment.end + 3000, y, -6000], [segment.end + 3000, y, 6000], [segment.start - 3000, y, 6000]], [0, 1, 0], built.surfaces);
    this.root.add(built.group);
    this.built.set(segment.index, built);
  }

  private decorate(built: Built): void {
    const { segment } = built;
    let state = segment.index * 7919 + this.seed;
    const random = () => (state = (state * 16807) % 2147483647) / 2147483647;
    for (let x = segment.start + 400; x < segment.end; x += 900 + random() * 900) {
      const side = random() < 0.5 ? -1 : 1;
      if (segment.theme === 0 && this.treeModel) {
        // Trees on islands beside the track.
        const z = side * (1700 + random() * 2500);
        const y = segment.exitTop - 300 - random() * 900;
        const island = new THREE.Mesh(tiledBox(700, 200, 700), [this.material('grassEdge'), this.material('grassEdge'), this.material('grassTop'), this.material('dirt'), this.material('grassEdge'), this.material('grassEdge')]);
        island.position.set(x, y - 100, z);
        const tree = instantiateSm64(this.treeModel);
        const height = new THREE.Box3().setFromObject(tree).getSize(new THREE.Vector3()).y || 1;
        tree.scale.multiplyScalar((380 + random() * 220) / height);
        tree.position.set(x, y, z);
        built.group.add(island, tree);
      } else if (segment.theme === 1) {
        const height = 1800 + random() * 2400;
        const tower = new THREE.Mesh(new THREE.CylinderGeometry(260, 300, height, 14), this.material('brick'));
        tower.position.set(x, segment.exitTop - 1800 + height / 2, side * (2000 + random() * 3000));
        const roof = new THREE.Mesh(new THREE.ConeGeometry(340, 460, 14), new THREE.MeshStandardMaterial({ color: 0xc0392b, roughness: 0.6 }));
        roof.position.set(tower.position.x, tower.position.y + height / 2 + 230, tower.position.z);
        built.group.add(tower, roof);
      } else if (segment.theme === 2) {
        const size = 300 + random() * 600;
        const cube = new THREE.Mesh(tiledBox(size, size, size, 200), this.material('checker', PASTELS[Math.floor(random() * PASTELS.length)]));
        cube.position.set(x, segment.exitTop + (random() - 0.3) * 2400, side * (1500 + random() * 3500));
        cube.rotation.set(random() * 3, random() * 3, 0);
        cube.userData.spin = 0.2 + random() * 0.5;
        built.spinners.push(cube);
        built.group.add(cube);
      }
    }
  }

  private retire(index: number): void {
    const built = this.built.get(index);
    if (!built) return;
    this.world.removeSurfaces(built.surfaces);
    for (const face of built.surfaces) this.pillarFaces.delete(face);
    built.group.traverse(node => { if (node instanceof THREE.Mesh && !node.userData.sharedSm64Asset) node.geometry.dispose(); });
    built.group.removeFromParent();
    this.built.delete(index);
  }

  // ------------------------------------------------------------- per tick

  beforeStep(controller: ControllerSnapshot): StepOverride | void {
    if (!this.track || this.state !== 'running') return { controller: emptyController(), cameraYaw: 0xc000 };
    // Always full speed ahead (+X); the player only steers and uses buttons.
    const steer = Math.max(-1, Math.min(1, controller.stickX / 64)) * 0.55;
    const x = steer * 64, y = Math.sqrt(Math.max(0, 64 * 64 - x * x));
    // Camera yaw 0xC000: stick-up runs toward +X, stick-right toward +Z.
    return { controller: { ...controller, stickX: x, stickY: y, stickMag: 64 }, cameraYaw: 0xc000 };
  }

  afterStep(): void {
    // Assets may still be loading: the run begins once the track exists.
    if (!this.track) return;
    const m = this.host.mario;
    this.ticks++;
    if (this.state === 'ready' && this.ticks >= COUNTDOWN) {
      this.state = 'running';
      this.host.toast('GO!', 900);
      this.host.sfx.startMusic(courseMusic(MUSIC[0]));
    }
    if (this.state === 'over') return;
    if (this.state === 'running') {
      this.runTicks++;
      this.recording.push(recordFrame(m));
      this.glanceOffPillars();
      this.stepHazards();
    }
    if (!(m.action & ACT_FLAG_AIR) && m.floor && !this.isCatchFloor(m.floorHeight)) this.lastTop = m.floorHeight;
    if (m.pos[1] < this.lastTop - 1300) { this.gameOver(); return; }
    this.distance = Math.max(this.distance, (m.pos[0] - ORIGIN_X) / 100);
    this.stream(m.pos[0]);
    for (const built of this.built.values()) {
      for (const coin of built.coins) {
        if (coin.taken || Math.abs(coin.pos[0] - m.pos[0]) > 200) continue;
        if (Math.hypot(coin.pos[0] - m.pos[0], coin.pos[1] - (m.pos[1] + 70), coin.pos[2] - m.pos[2]) < 130) {
          coin.taken = true;
          coin.group.visible = false;
          this.coins++;
          this.host.addCoins(1);
          this.host.sfx.coin();
          this.host.particles.sparks(coin.pos, 6);
        }
      }
    }
    const current = [...this.built.values()].find(b => m.pos[0] >= b.segment.start && m.pos[0] < b.segment.end);
    const theme = current?.segment.theme ?? 0;
    if (theme !== this.theme) {
      if (this.theme >= 0) {
        this.host.toast(THEMES[theme].name.toUpperCase(), 2200);
        this.host.sfx.startMusic(courseMusic(MUSIC[theme]));
      }
      this.theme = theme;
    }
  }

  /** Clipping a pillar nudges Mario around it instead of stopping the run dead. */
  private glanceOffPillars(): void {
    const m = this.host.mario;
    const block = m.wall ? this.pillarFaces.get(m.wall) : undefined;
    if (!block || m.forwardVel > 20) { this.blocked = 0; return; }
    if (block.kind === 'hurdle') {
      // Stuck at a hurdle: scramble over it (slower than jumping it cleanly).
      if (++this.blocked >= 10 && !(m.action & ACT_FLAG_AIR)) {
        this.blocked = 0;
        m.setAction(ACT_JUMP, 0);
        m.vel[1] = 46;
        m.forwardVel = 18;
        this.host.particles.dust(m.pos, 6);
      }
      return;
    }
    const side = m.pos[2] >= block.z ? 1 : -1;
    m.pos[2] += side * 26;
    m.forwardVel = Math.max(m.forwardVel, 22);
    if (this.ticks % 6 === 0) this.host.particles.dust(m.pos, 4);
  }

  /** Boulders sweep across the track; touching one knocks Mario back. */
  private stepHazards(): void {
    const m = this.host.mario;
    for (const built of this.built.values()) {
      for (const entry of built.hazards) {
        const { hazard, mesh } = entry;
        const angle = (this.ticks / hazard.period + hazard.phase) * Math.PI * 2;
        const z = Math.sin(angle) * hazard.sweep;
        mesh.rotation.x += (z - entry.z) / hazard.radius;
        entry.z = z;
        mesh.position.set(hazard.x, hazard.top + hazard.radius, z);
        const dx = m.pos[0] - hazard.x, dy = m.pos[1] + 80 - mesh.position.y, dz = m.pos[2] - z;
        if (Math.hypot(dx, dy, dz) < hazard.radius + 60 && m.takeDamage(1, [hazard.x - 200, mesh.position.y, z])) {
          m.hurtCounter = 0;
          this.hits++;
          this.host.particles.sparks([m.pos[0], m.pos[1] + 80, m.pos[2]], 12);
          this.host.sfx.bonk();
        }
      }
    }
  }

  private isCatchFloor(height: number): boolean {
    return height < this.lastTop - 2500;
  }

  private gameOver(): void {
    this.state = 'over';
    const score = Math.floor(this.distance);
    const record = score > this.best;
    if (record) {
      this.best = score;
      storage(this.bestKey(), score);
      // The best daily run becomes the ghost for today's track.
      if (this.mode === 'daily') storage(`${GHOST_KEY}.${this.seed}`, this.recording);
    }
    this.host.sfx.death();
    this.overCard.replaceChildren(
      el('strong', '', record ? 'NEW RECORD!' : 'YOU FELL'),
      el('p', 'xp-gameover-score', `${score.toLocaleString()} m`),
      el('p', '', `${this.coins} coins · best ${Math.floor(this.best).toLocaleString()} m${this.mode === 'daily' ? ' today' : ''}`),
      el('small', '', 'SPACE · RUN AGAIN'),
    );
    this.overCard.hidden = false;
  }

  frame(dt: number): void {
    if (!this.track) return;
    const { camera, renderPos } = this.host;
    // A low chase camera that leans into the run and lags a little on height.
    const desired = new THREE.Vector3(renderPos.x - 980, renderPos.y + 430, renderPos.z * 0.55);
    if (this.camPos.lengthSq() === 0 || this.camPos.distanceTo(desired) > 3000) {
      this.camPos.copy(desired);
      this.camLook.set(renderPos.x + 700, renderPos.y + 80, renderPos.z * 0.8);
    }
    const k = 1 - Math.exp(-dt * 6);
    this.camPos.x = desired.x;
    this.camPos.y += (desired.y - this.camPos.y) * (this.state === 'over' ? 0 : k * 0.6);
    this.camPos.z += (desired.z - this.camPos.z) * k;
    this.camLook.lerp(new THREE.Vector3(renderPos.x + 700, (this.state === 'over' ? this.lastTop : renderPos.y) + 80, renderPos.z * 0.8), k);
    camera.position.copy(this.camPos);
    camera.up.set(0, 1, 0);
    camera.lookAt(this.camLook);
    this.sky.update(camera, this.host.time);
    const t = Math.max(0, this.theme);
    this.sky.setColors(SKIES[t], Math.min(1, dt * 0.8));
    if (this.host.scene.fog instanceof THREE.Fog) this.host.scene.fog.color.lerp(new THREE.Color(FOG[t]), Math.min(1, dt * 0.8));
    if (this.ghost) {
      const showing = this.state === 'running' && this.ghost.show(this.ghostFrames, this.runTicks, dt, camera);
      if (!showing) this.ghost.root.visible = false;
    }
    for (const built of this.built.values()) {
      for (const spinner of built.spinners) spinner.rotation.y += dt * (spinner.userData.spin ?? 0.2);
      for (const coin of built.coins) if (!coin.taken) coin.group.rotation.y += dt * 4;
    }
    this.readouts[0]?.(`${Math.floor(this.distance).toLocaleString()} m`);
    this.readouts[1]?.(`${Math.floor(Math.max(this.best, this.distance)).toLocaleString()} m`);
    this.readouts[2]?.(String(this.coins));
    this.readouts[3]?.(THEMES[t].name);
  }

  debug(): Record<string, unknown> {
    return {
      seed: this.seed,
      mode: this.mode,
      ghostFrames: this.ghostFrames.length,
      hits: this.hits,
      state: this.state,
      distance: this.distance,
      coins: this.coins,
      runs: this.runs,
      built: [...this.built.keys()].sort((a, b) => a - b),
      kinds: [...this.built.values()].map(b => b.segment.kind),
      theme: this.theme,
      surfaces: this.world.surfaces.length,
    };
  }
}
