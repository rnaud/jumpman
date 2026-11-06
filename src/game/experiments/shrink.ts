import * as THREE from 'three';
import { CollisionWorld } from '../../sim/collision';
import type { Vec3 } from '../../sim/math';
import { SkyDome } from './sky';
import type { Experiment, ExperimentHost } from './types';
import { dock, segmented, titleCard, type Segmented } from './ui';

/** World scale factors: the world grows (Mario shrinks) or the world shrinks (Mario grows). */
export const SIZES = [
  { id: 'tiny', label: 'Tiny', world: 8 },
  { id: 'small', label: 'Small', world: 3 },
  { id: 'normal', label: 'Normal', world: 1 },
  { id: 'giant', label: 'Giant', world: 0.25 },
] as const;
type SizeId = (typeof SIZES)[number]['id'];

/** Far from the original course, so its (hidden) objects never meet Mario. */
const OFFSET = new THREE.Vector3(400000, 0, 0);
const TRANSITION = 0.9;

/** Map a point from the original course into a world scaled by `k`. */
export function toScaled(p: Vec3, k: number): Vec3 {
  return [OFFSET.x + p[0] * k, OFFSET.y + p[1] * k, OFFSET.z + p[2] * k];
}
export function fromScaled(p: Vec3, k: number): Vec3 {
  return [(p[0] - OFFSET.x) / k, (p[1] - OFFSET.y) / k, (p[2] - OFFSET.z) / k];
}

/**
 * Shrink Ray: Mario keeps SM64's exact physics while the whole course is
 * rescaled around him, so a blade of grass becomes a platform (Tiny) or the
 * mountain becomes a step (Giant). Collision is rebuilt for every size.
 */
export class ShrinkRayExperiment implements Experiment {
  readonly id = 'shrink';
  readonly title = 'Shrink Ray';
  readonly course = null;
  readonly replacesCourse = true;
  private host!: ExperimentHost;
  private readonly copy = new THREE.Group();
  private readonly worlds = new Map<number, CollisionWorld>();
  private scale = 1;
  private size: SizeId = 'normal';
  private seg!: Segmented<SizeId>;
  private sky!: SkyDome;
  private transition: { from: number; to: number; t: number; anchor: Vec3 } | null = null;
  private saved: { far: number; fog: THREE.Scene['fog'] } | null = null;
  private resizes = 0;

  get timeScale(): number { return this.transition ? 0 : 1; }

  enter(host: ExperimentHost): void {
    this.host = host;
    this.saved = { far: host.camera.far, fog: host.scene.fog };
    // A static copy of the course: level geometry and scenery, but no objects.
    const source = host.courseScene;
    const keep = (node: THREE.Object3D) => /_geo_|tree|fence|post|bridge|platform/i.test(node.name) && !/coin|star|bobomb|goomba|koopa|chomp/i.test(node.name);
    source.traverse(node => {
      if (node.parent && node !== source && keep(node) && !keepAncestor(node, keep, source)) {
        const clone = node.clone(true);
        node.parent.updateWorldMatrix(true, false);
        clone.applyMatrix4(node.parent.matrixWorld);
        this.copy.add(clone);
      }
    });
    this.copy.name = 'shrink-ray-course';
    host.scene.add(this.copy);
    this.sky = new SkyDome({ top: 0x2f7fd6, mid: 0x8fcdf5, bottom: 0xe9f6ff }, 1_500_000, 0.6);
    host.scene.add(this.sky.mesh);
    titleCard(host.ui, '07', 'Shrink Ray', 'Same Mario, same physics — a different world size. Tiny makes grass a jungle; Giant makes the mountain a step.', [
      ['1 2 3 4', 'tiny · small · normal · giant'], ['WASD · Space', 'run · jump'],
    ]);
    const bar = dock(host.ui);
    this.seg = segmented(SIZES.map((size, i) => ({ value: size.id, label: `${size.label} ${size.world >= 1 ? `×${size.world}` : '×¼'}`, key: String(i + 1) })), this.size, size => this.resize(size));
    bar.append(this.seg.element);
    this.applyWorld(1, [...host.mario.pos]);
    host.toast('PICK A SIZE · 1 TINY · 4 GIANT', 2600);
  }

  exit(): void {
    this.copy.removeFromParent();
    this.copy.clear();
    this.sky.dispose();
    if (this.saved) {
      this.host.camera.far = this.saved.far;
      this.host.camera.updateProjectionMatrix();
      this.host.scene.fog = this.saved.fog;
    }
    this.host.restoreMarioWorld();
  }

  keydown(event: KeyboardEvent): boolean {
    const index = ['Digit1', 'Digit2', 'Digit3', 'Digit4'].indexOf(event.code);
    if (index < 0) return false;
    this.resize(SIZES[index].id);
    return true;
  }

  private worldFor(k: number): CollisionWorld {
    let world = this.worlds.get(k);
    if (!world) {
      const source = this.host.playground.collision;
      world = new CollisionWorld(1024, Math.max(source.horizontalBoundary * k, 1024) + OFFSET.x + 4096);
      for (const s of source.surfaces) {
        world.addTriangle(toScaled(s.v1, k), toScaled(s.v2, k), toScaled(s.v3, k), { type: s.type, force: s.force, flags: s.flags, room: s.room });
      }
      world.waterBoxes = source.waterBoxes.map(([id, x1, z1, x2, z2, y]) => [id, OFFSET.x + x1 * k, OFFSET.z + z1 * k, OFFSET.x + x2 * k, OFFSET.z + z2 * k, OFFSET.y + y * k]);
      world.ceilingLimit = Math.max(20000, 20000 * k + OFFSET.y);
      world.finalize();
      this.worlds.set(k, world);
    }
    return world;
  }

  /** Put Mario (at original-course point `anchor`) into the world scaled by `k`. */
  private applyWorld(k: number, anchor: Vec3): void {
    const { host } = this;
    const world = this.worldFor(k);
    const spawn = host.playground.spawn;
    host.setMarioWorld(world, { ...spawn, pos: toScaled(spawn.pos, k) }, OFFSET.y + (host.playground.deathHeight ?? -1600) * k, []);
    host.placeMario(toScaled(anchor, k), host.mario.faceAngle[1]);
    this.copy.scale.setScalar(k);
    this.copy.position.copy(OFFSET);
    this.scale = k;
    host.camera.far = Math.max(40000, 40000 * k);
    host.camera.updateProjectionMatrix();
    if (this.saved?.fog instanceof THREE.Fog) host.scene.fog = new THREE.Fog(this.saved.fog.color, this.saved.fog.near * k, this.saved.fog.far * k);
  }

  private resize(size: SizeId): void {
    const target = SIZES.find(entry => entry.id === size)!.world;
    this.seg.set(size);
    if (target === this.scale || this.transition) return;
    this.size = size;
    this.transition = { from: this.scale, to: target, t: 0, anchor: fromScaled(this.host.mario.pos, this.scale) };
    this.resizes++;
    this.host.sfx.teleport();
    this.host.toast(target > 1 ? 'SHRINKING…' : target < 1 ? 'GROWING…' : 'BACK TO NORMAL', 1000);
  }

  frame(dt: number): void {
    const { camera } = this.host;
    const tr = this.transition;
    if (tr) {
      tr.t = Math.min(1, tr.t + dt / TRANSITION);
      const e = tr.t * tr.t * (3 - 2 * tr.t);
      // Scale the copy about Mario, so he stays put while the world swells or shrinks.
      const k = Math.exp(Math.log(tr.from) + (Math.log(tr.to) - Math.log(tr.from)) * e);
      const mario = new THREE.Vector3(...this.host.mario.pos);
      const anchor = new THREE.Vector3(...tr.anchor);
      this.copy.scale.setScalar(k);
      this.copy.position.copy(mario).addScaledVector(anchor, -k);
      if (tr.t >= 1) {
        this.transition = null;
        this.applyWorld(tr.to, tr.anchor);
      }
    }
    this.sky.update(camera, this.host.time);
  }

  debug(): Record<string, unknown> {
    return { size: this.size, scale: this.scale, transitioning: Boolean(this.transition), resizes: this.resizes, copyNodes: this.copy.children.length, surfaces: this.worldFor(this.scale).surfaces.length };
  }
}

function keepAncestor(node: THREE.Object3D, keep: (node: THREE.Object3D) => boolean, root: THREE.Object3D): boolean {
  for (let parent = node.parent; parent && parent !== root; parent = parent.parent) if (keep(parent)) return true;
  return false;
}
