import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { CollisionWorld, type Surface } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { instantiateSm64, loadSm64Asset, SM64_MODELS } from '../sm64Assets';
import { addFace, sm64Texture } from './geometry';
import { makeRng } from './marioCrowd';
import { SkyDome } from './sky';
import type { Experiment, ExperimentHost } from './types';
import { dock, readout, titleCard } from './ui';

export const PLANET_RADIUS = 2200;
/** Features closer than this (along the surface) get collision in Mario's frame. */
const COLLIDE_RANGE = 2600;
const UP = new THREE.Vector3(0, 1, 0);

interface Pillar { dir: THREE.Vector3; radius: number; height: number; mesh: THREE.Object3D }
interface PlanetCoin { dir: THREE.Vector3; height: number; mesh: THREE.Object3D; taken: boolean }

/**
 * Map a direction on the planet into Mario's flat frame (log map): the arc
 * length becomes flat distance, so gravity and SM64 physics stay untouched.
 */
export function toFlat(dir: THREE.Vector3, inverse: THREE.Quaternion, radius = PLANET_RADIUS): { x: number; z: number; angle: number } {
  const local = dir.clone().applyQuaternion(inverse);
  const angle = Math.acos(THREE.MathUtils.clamp(local.y, -1, 1));
  const h = Math.hypot(local.x, local.z);
  if (h < 1e-6) return { x: 0, z: 0, angle };
  return { x: (local.x / h) * angle * radius, z: (local.z / h) * angle * radius, angle };
}

/**
 * Tiny Planet: run all the way around a little world. Mario's physics run on
 * a flat floor; each tick the planet turns beneath him by exactly the arc he
 * moved, and he is shifted back to the centre of his frame, so "down" always
 * points at the planet's core.
 */
export class TinyPlanetExperiment implements Experiment {
  readonly id = 'planet';
  readonly title = 'Tiny Planet';
  readonly course = null;
  readonly replacesCourse = true;
  private host!: ExperimentHost;
  private readonly world = new CollisionWorld(1024, 1e9);
  private readonly ground = new Set<Surface>();
  private featureSurfaces = new Set<Surface>();
  /** Planet orientation relative to Mario's frame: planet = frame * orientation. */
  private orientation = new THREE.Quaternion();
  private previousOrientation = new THREE.Quaternion();
  private readonly planet = new THREE.Group();
  private readonly spinning = new THREE.Group();
  private pillars: Pillar[] = [];
  private coins: PlanetCoin[] = [];
  private sky!: SkyDome;
  private saved: { fog: THREE.Scene['fog']; background: THREE.Scene['background']; far: number } | null = null;
  private travelled = 0;
  private laps = 0;
  private collected = 0;
  private readouts: Array<(value: string) => void> = [];
  private readonly startDir = new THREE.Vector3(0, 1, 0);
  private lapArmed = false;

  async enter(host: ExperimentHost): Promise<void> {
    this.host = host;
    this.saved = { fog: host.scene.fog, background: host.scene.background, far: host.camera.far };
    host.scene.fog = null;
    host.scene.background = null;
    this.sky = new SkyDome({ top: 0x05051a, mid: 0x1b2a6b, bottom: 0x6a5acd }, 90000, 0);
    host.scene.add(this.sky.mesh);
    // An infinite flat floor in Mario's frame; the curvature lives in rendering.
    addFace(this.world, [[-40000, 0, -40000], [40000, 0, -40000], [40000, 0, 40000], [-40000, 0, 40000]], [0, 1, 0], this.ground);
    this.world.finalize();
    host.setMarioWorld(this.world, { pos: [0, 0, 0], faceYaw: 0 }, -1e9, []);
    host.placeMario([0, 0, 0], 0);
    let tree: GLTF | null = null, coin: GLTF | null = null;
    try { [tree, coin] = await Promise.all([loadSm64Asset(SM64_MODELS.tree), loadSm64Asset(SM64_MODELS.coin)]); } catch { /* decor optional */ }
    this.buildPlanet(tree, coin);
    titleCard(host.ui, '08', 'Tiny Planet', 'A world small enough to run around in fifteen seconds. Gravity always pulls toward its core.', [
      ['WASD · Space', 'run · jump'], ['Pillars', 'climb them for the high coins'],
    ]);
    const bar = dock(host.ui);
    bar.classList.add('xp-dock-stats');
    this.readouts = [readout(bar, 'Laps'), readout(bar, 'Travelled'), readout(bar, 'Coins')];
    host.camera.far = 120000;
    host.camera.updateProjectionMatrix();
    host.toast('TINY PLANET · RUN ALL THE WAY AROUND', 2600);
  }

  private buildPlanet(tree: GLTF | null, coin: GLTF | null): void {
    const { host } = this;
    const R = PLANET_RADIUS;
    const grass = sm64Texture('grassTop');
    if (grass) {
      grass.repeat.set(24, 12);
    }
    const sphere = new THREE.Mesh(new THREE.SphereGeometry(R, 96, 64), new THREE.MeshStandardMaterial({ map: grass, color: 0xbfe8a0, roughness: 0.95 }));
    sphere.receiveShadow = true;
    this.spinning.add(sphere);
    const random = makeRng(0xb1a5e7);
    const randomDir = () => new THREE.Vector3(random() * 2 - 1, random() * 2 - 1, random() * 2 - 1).normalize();
    const place = (object: THREE.Object3D, dir: THREE.Vector3, height: number) => {
      object.position.copy(dir).multiplyScalar(R + height);
      object.quaternion.setFromUnitVectors(UP, dir);
      object.rotateY(random() * Math.PI * 2);
      this.spinning.add(object);
    };
    const awayFromStart = (dir: THREE.Vector3) => dir.angleTo(this.startDir) > 0.35;
    // Stone pillars to climb, each topped with coins.
    for (let i = 0; i < 9; i++) {
      let dir = randomDir();
      while (!awayFromStart(dir)) dir = randomDir();
      const radius = 160 + random() * 160, height = 180 + random() * 320;
      const pillar = new THREE.Mesh(new THREE.CylinderGeometry(radius, radius * 1.08, height, 18), new THREE.MeshStandardMaterial({ map: sm64Texture('flagstone'), roughness: 0.85 }));
      pillar.geometry.translate(0, height / 2, 0);
      pillar.castShadow = pillar.receiveShadow = true;
      place(pillar, dir, 0);
      this.pillars.push({ dir, radius, height, mesh: pillar });
      if (coin) for (let k = 0; k < 3; k++) this.addCoin(coin, dir, height + 90 + k * 70, place);
    }
    // Trees and a belt of coins around the equator.
    if (tree) {
      for (let i = 0; i < 26; i++) {
        let dir = randomDir();
        while (!awayFromStart(dir) || this.pillars.some(p => p.dir.angleTo(dir) < 0.2)) dir = randomDir();
        const model = instantiateSm64(tree);
        const height = new THREE.Box3().setFromObject(model).getSize(new THREE.Vector3()).y || 1;
        model.scale.multiplyScalar((260 + random() * 160) / height);
        place(model, dir, 0);
      }
    }
    if (coin) {
      for (let i = 0; i < 24; i++) {
        const a = (i / 24) * Math.PI * 2;
        this.addCoin(coin, new THREE.Vector3(Math.cos(a), 0.05 * Math.sin(a * 3), Math.sin(a)).normalize(), 90, place);
      }
    }
    // A little moon and a ring of stars, fixed to the planet so they wheel past as you run.
    const moon = new THREE.Mesh(new THREE.SphereGeometry(620, 40, 24), new THREE.MeshStandardMaterial({ color: 0xe8e2d0, roughness: 1, map: sm64Texture('stone') }));
    moon.position.set(5200, 3200, -3800);
    this.spinning.add(moon);
    const stars = new THREE.BufferGeometry();
    const positions: number[] = [];
    for (let i = 0; i < 1400; i++) {
      const d = randomDir().multiplyScalar(60000 + random() * 20000);
      positions.push(d.x, d.y, d.z);
    }
    stars.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    this.spinning.add(new THREE.Points(stars, new THREE.PointsMaterial({ color: 0xffffff, size: 140, sizeAttenuation: true, fog: false })));
    this.planet.add(this.spinning);
    this.planet.position.set(0, -R, 0);
    this.planet.name = 'tiny-planet';
    host.scene.add(this.planet);
    this.syncFeatures();
  }

  private addCoin(model: GLTF, dir: THREE.Vector3, height: number, place: (o: THREE.Object3D, d: THREE.Vector3, h: number) => void): void {
    const coin = instantiateSm64(model);
    coin.scale.multiplyScalar(0.9);
    coin.traverse(node => { if (node instanceof THREE.Mesh) node.castShadow = false; });
    place(coin, dir, height);
    this.coins.push({ dir, height, mesh: coin, taken: false });
  }

  exit(): void {
    this.planet.removeFromParent();
    this.planet.traverse(node => {
      if ((node instanceof THREE.Mesh || node instanceof THREE.Points) && !node.userData.sharedSm64Asset) {
        node.geometry.dispose();
        (node.material as THREE.Material).dispose();
      }
    });
    this.sky.dispose();
    const { host } = this;
    if (this.saved) {
      host.scene.fog = this.saved.fog;
      host.scene.background = this.saved.background;
      host.camera.far = this.saved.far;
      host.camera.updateProjectionMatrix();
    }
    host.restoreMarioWorld();
  }

  afterStep(): void {
    const m = this.host.mario;
    const dx = m.pos[0], dz = m.pos[2];
    const distance = Math.hypot(dx, dz);
    this.previousOrientation.copy(this.orientation);
    if (distance > 0.01) {
      // Roll the planet under Mario by the arc he just covered.
      const axis = new THREE.Vector3().crossVectors(UP, new THREE.Vector3(dx / distance, 0, dz / distance)).normalize();
      const roll = new THREE.Quaternion().setFromAxisAngle(axis, distance / PLANET_RADIUS);
      // planet-in-frame rotates the opposite way to Mario's motion.
      this.orientation.premultiply(roll.invert());
      this.host.shiftMario(-dx, 0, -dz);
      this.travelled += distance;
    }
    this.syncFeatures();
    this.collect();
    this.countLaps();
  }

  /** Rebuild collision for nearby pillars in Mario's flat frame. */
  private syncFeatures(): void {
    this.world.removeSurfaces(this.featureSurfaces);
    this.featureSurfaces = new Set();
    const inverse = this.orientation.clone();
    for (const pillar of this.pillars) {
      const dir = pillar.dir.clone().applyQuaternion(inverse);
      const flat = toFlat(dir, new THREE.Quaternion());
      if (Math.hypot(flat.x, flat.z) > COLLIDE_RANGE) continue;
      const segments = 14;
      const ring: Vec3[] = [];
      for (let i = 0; i < segments; i++) {
        const a = (i / segments) * Math.PI * 2;
        ring.push([flat.x + Math.sin(a) * pillar.radius, pillar.height, flat.z + Math.cos(a) * pillar.radius]);
      }
      addFace(this.world, ring, [0, 1, 0], this.featureSurfaces);
      for (let i = 0; i < segments; i++) {
        const a = ring[i], b = ring[(i + 1) % segments];
        const mid = ((i + 0.5) / segments) * Math.PI * 2;
        addFace(this.world, [a, b, [b[0], -50, b[2]], [a[0], -50, a[2]]], [Math.sin(mid), 0, Math.cos(mid)], this.featureSurfaces);
      }
    }
  }

  private collect(): void {
    const m = this.host.mario;
    for (const coin of this.coins) {
      if (coin.taken) continue;
      const local = toFlat(coin.dir.clone().applyQuaternion(this.orientation), new THREE.Quaternion());
      if (Math.hypot(local.x - m.pos[0], local.z - m.pos[2]) < 110 && Math.abs(coin.height - (m.pos[1] + 70)) < 120) {
        coin.taken = true;
        coin.mesh.visible = false;
        this.collected++;
        this.host.addCoins(1);
        this.host.sfx.coin();
        this.host.particles.sparks([m.pos[0], m.pos[1] + 80, m.pos[2]], 6);
      }
    }
  }

  /** A lap is leaving the start point by a quarter turn and coming back to it. */
  private countLaps(): void {
    const start = this.startDir.clone().applyQuaternion(this.orientation);
    const angle = start.angleTo(UP);
    if (angle > Math.PI * 0.8) this.lapArmed = true;
    if (this.lapArmed && angle < 0.12) {
      this.lapArmed = false;
      this.laps++;
      this.host.sfx.starSound();
      this.host.toast(`LAP ${this.laps} · ALL THE WAY AROUND!`, 1800);
    }
  }

  frame(dt: number, alpha: number): void {
    // Interpolate the planet's roll between physics ticks, like Mario's position.
    this.spinning.quaternion.slerpQuaternions(this.previousOrientation, this.orientation, alpha);
    this.sky.update(this.host.camera, this.host.time);
    for (const coin of this.coins) if (!coin.taken) coin.mesh.rotateY(dt * 3);
    this.readouts[0]?.(String(this.laps));
    this.readouts[1]?.(`${Math.round(this.travelled / 100).toLocaleString()} m`);
    this.readouts[2]?.(`${this.collected} / ${this.coins.length}`);
  }

  debug(): Record<string, unknown> {
    return {
      travelled: this.travelled,
      laps: this.laps,
      coins: this.collected,
      totalCoins: this.coins.length,
      pillars: this.pillars.length,
      featureSurfaces: this.featureSurfaces.size,
      startAngle: this.startDir.clone().applyQuaternion(this.orientation).angleTo(UP),
    };
  }
}
