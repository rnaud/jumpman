import * as THREE from 'three';
import type { CollisionWorld } from '../../sim/collision';
import { MarioRuntime, type SpawnPoint } from '../../sim/MarioRuntime';
import { coss, sins, toS16, type Vec3 } from '../../sim/math';
import { A_BUTTON, B_BUTTON, Z_TRIG, ACT_FLAG_AIR, type ControllerSnapshot } from '../../sim/state';
import { Sm64MarioView } from '../Sm64MarioView';

/** Mario's red (cap/shirt) and blue (overalls) display-list materials in the exported rig. */
const CAP_MATERIAL = '[1.0, 0.0, 0.0]';
const OVERALLS_MATERIAL = '[0.0, 0.0, 1.0]';

const tintCache = new Map<string, THREE.MeshStandardMaterial>();
function tinted(source: THREE.MeshStandardMaterial, color: number): THREE.MeshStandardMaterial {
  const key = `${source.uuid}:${color}`;
  let material = tintCache.get(key);
  if (!material) {
    material = source.clone();
    material.color.setHex(color);
    tintCache.set(key, material);
  }
  return material;
}

export interface CrowdLook {
  cap: number;
  overalls?: number;
}

export interface DriveInput {
  /** World yaw (SM64 s16) to run toward, or null to let go of the stick. */
  yaw: number | null;
  /** Stick magnitude 0..64. */
  mag?: number;
  /** Press A this frame. */
  jump?: boolean;
  /** Keep A held (higher jumps). */
  holdA?: boolean;
  punch?: boolean;
  crouch?: boolean;
  /** Keep Z held (crouch, backflip set-up). */
  holdZ?: boolean;
}

let viewTemplate: Promise<unknown> | null = null;

/**
 * One autonomous Mario: a full MarioRuntime fed by synthetic controller input
 * and rendered with the real SM64 rig and its native animation clips.
 */
export class CrowdMario {
  readonly runtime: MarioRuntime;
  readonly root = new THREE.Group();
  private readonly prevPos = new THREE.Vector3();
  private readonly renderPos = new THREE.Vector3();
  private readonly tintable: THREE.Mesh[] = [];
  private prevYaw = 0;
  private animationDebt = 0;
  private skip = 0;
  private wasAir = false;
  /** Frames since the agent last touched the ground after being airborne. */
  landedTicks = 99;

  private constructor(world: CollisionWorld, spawn: SpawnPoint, private readonly view: Sm64MarioView, look: CrowdLook) {
    this.runtime = new MarioRuntime(world, spawn);
    this.root.add(view.root);
    view.root.traverse(node => {
      if (!(node instanceof THREE.Mesh)) return;
      node.castShadow = false;
      node.receiveShadow = false;
      const name = (node.material as THREE.Material).name;
      if (name === CAP_MATERIAL || name === OVERALLS_MATERIAL) {
        node.userData.crowdBase = node.material;
        this.tintable.push(node);
      }
    });
    this.setLook(look);
    this.syncPrevious();
  }

  /** Warm the shared model/clip loads once, so later spawns are synchronous-fast. */
  static preload(): Promise<unknown> {
    viewTemplate ??= Sm64MarioView.load().catch(error => { viewTemplate = null; throw error; });
    return viewTemplate;
  }

  static async create(world: CollisionWorld, spawn: SpawnPoint, look: CrowdLook): Promise<CrowdMario> {
    await CrowdMario.preload();
    return new CrowdMario(world, spawn, await Sm64MarioView.load(), look);
  }

  get pos(): Vec3 { return this.runtime.pos; }
  get interpolated(): THREE.Vector3 { return this.renderPos; }
  /** Position before the latest 30 Hz step. */
  get previous(): THREE.Vector3 { return this.prevPos; }

  setLook(look: CrowdLook): void {
    for (const node of this.tintable) {
      const base = node.userData.crowdBase as THREE.MeshStandardMaterial;
      const color = base.name === CAP_MATERIAL ? look.cap : look.overalls;
      node.material = color === undefined ? base : tinted(base, color);
    }
  }

  syncPrevious(): void {
    this.prevPos.fromArray(this.runtime.pos);
    this.renderPos.copy(this.prevPos);
    this.prevYaw = this.runtime.faceAngle[1];
  }

  teleport(pos: Vec3, faceYaw = this.runtime.faceAngle[1]): void {
    const m = this.runtime;
    m.spawn = { pos: [...pos], faceYaw };
    m.resetToSpawn();
    this.syncPrevious();
  }

  /** Advance one 30 Hz frame with synthetic stick and buttons. */
  drive(input: DriveInput): void {
    const mag = input.yaw === null ? 0 : Math.max(0, Math.min(64, input.mag ?? 64));
    const yaw = input.yaw ?? 0;
    let down = 0;
    let pressed = 0;
    if (input.jump) { pressed |= A_BUTTON; down |= A_BUTTON; }
    if (input.holdA) down |= A_BUTTON;
    if (input.punch) { pressed |= B_BUTTON; down |= B_BUTTON; }
    if (input.crouch) { pressed |= Z_TRIG; down |= Z_TRIG; }
    if (input.holdZ) down |= Z_TRIG;
    const controller: ControllerSnapshot = {
      // With a zero camera yaw, intendedYaw = atan2s(-stickY, stickX) = yaw.
      stickX: mag * sins(yaw),
      stickY: -mag * coss(yaw),
      stickMag: mag,
      buttonDown: down,
      buttonPressed: pressed,
    };
    this.prevPos.fromArray(this.runtime.pos);
    this.prevYaw = this.runtime.faceAngle[1];
    this.runtime.step(controller, 0);
    const air = Boolean(this.runtime.action & ACT_FLAG_AIR);
    if (this.wasAir && !air) this.landedTicks = 0;
    else if (this.landedTicks < 99) this.landedTicks++;
    this.wasAir = air;
    // Crowd Marios never die; knockbacks still play out in full.
    this.runtime.hurtCounter = 0;
    this.runtime.health = 0x880;
  }

  /**
   * Interpolate and animate. Distant Marios animate at a reduced rate to keep
   * a hundred rigs affordable; their timing stays correct via accumulated dt.
   */
  render(alpha: number, dt: number, cameraDistance: number): void {
    const m = this.runtime;
    this.renderPos.set(
      THREE.MathUtils.lerp(this.prevPos.x, m.pos[0], alpha),
      THREE.MathUtils.lerp(this.prevPos.y, m.pos[1], alpha),
      THREE.MathUtils.lerp(this.prevPos.z, m.pos[2], alpha),
    );
    const yaw = toS16(this.prevYaw + toS16(m.faceAngle[1] - this.prevYaw) * alpha);
    this.animationDebt += dt;
    const interval = cameraDistance > 9000 ? 4 : cameraDistance > 4500 ? 2 : 1;
    if (++this.skip < interval) {
      this.view.root.position.copy(this.renderPos);
      return;
    }
    this.skip = 0;
    this.view.update(m, this.animationDebt, this.renderPos, yaw, interval === 1);
    this.animationDebt = 0;
  }

  faceCamera(camera: THREE.Camera): void { this.view.faceCamera(camera); }

  dispose(): void {
    this.root.removeFromParent();
  }
}

/** One instanced blob shadow per crowd member (one draw call for the whole crowd). */
export class CrowdShadows {
  readonly mesh: THREE.InstancedMesh;
  private readonly matrix = new THREE.Matrix4();
  private readonly quaternion = new THREE.Quaternion();
  private readonly up = new THREE.Vector3(0, 0, 1);
  private readonly normal = new THREE.Vector3();
  private readonly scale = new THREE.Vector3();
  private readonly position = new THREE.Vector3();

  constructor(capacity: number) {
    this.mesh = new THREE.InstancedMesh(
      new THREE.CircleGeometry(48, 18),
      new THREE.MeshBasicMaterial({ color: 0x000000, transparent: true, opacity: 0.32, depthWrite: false, polygonOffset: true, polygonOffsetFactor: -2 }),
      capacity,
    );
    this.mesh.frustumCulled = false;
    this.mesh.count = 0;
    this.mesh.renderOrder = 1;
  }

  update(bodies: readonly CrowdMario[]): void {
    let count = 0;
    for (const body of bodies) {
      const m = body.runtime;
      if (!m.floor || count >= this.mesh.instanceMatrix.count) continue;
      const height = Math.max(0, body.interpolated.y - m.floorHeight);
      const size = Math.max(0.35, 1 - height / 1100);
      this.normal.fromArray(m.floor.normal);
      this.quaternion.setFromUnitVectors(this.up, this.normal);
      this.position.set(body.interpolated.x, m.floorHeight + 2, body.interpolated.z);
      this.scale.setScalar(size);
      this.matrix.compose(this.position, this.quaternion, this.scale);
      this.mesh.setMatrixAt(count++, this.matrix);
    }
    this.mesh.count = count;
    this.mesh.instanceMatrix.needsUpdate = true;
  }

  dispose(): void {
    this.mesh.removeFromParent();
    this.mesh.geometry.dispose();
    (this.mesh.material as THREE.Material).dispose();
  }
}

interface BatchGroup {
  batch: THREE.BatchedMesh;
  geometryIds: Map<THREE.BufferGeometry, number>;
}

interface TrackedPart {
  mesh: THREE.Mesh;
  group: BatchGroup;
  instance: number;
  /** Ancestors up to the body root, checked each frame for switch-node visibility. */
  chain: THREE.Object3D[];
}

const RENDER_LAYER = 0;
const HIDDEN_LAYER = 7;

/**
 * Draw a whole crowd with one draw call per Mario material instead of one per
 * body part: each part of each Mario becomes an instance in a BatchedMesh,
 * fed the part's animated world matrix and its (possibly tinted) colour.
 * The original rigs keep animating but are moved to a layer the camera skips.
 */
export class CrowdBatcher {
  readonly root = new THREE.Group();
  private readonly groups = new Map<string, BatchGroup>();
  private readonly parts = new Map<CrowdMario, TrackedPart[]>();
  private readonly color = new THREE.Color();

  constructor(private readonly capacity: number) {
    this.root.name = 'crowd-batches';
  }

  get drawCalls(): number { return this.groups.size; }

  add(body: CrowdMario): void {
    const meshes: THREE.Mesh[] = [];
    body.root.traverse(node => { if (node instanceof THREE.Mesh) meshes.push(node); });
    const tracked: TrackedPart[] = [];
    for (const mesh of meshes) {
      const group = this.groupFor(mesh, meshes);
      if (!group) continue;
      const geometryId = group.geometryIds.get(mesh.geometry)!;
      const instance = group.batch.addInstance(geometryId);
      const chain: THREE.Object3D[] = [];
      for (let node: THREE.Object3D | null = mesh; node && node !== body.root; node = node.parent) chain.push(node);
      mesh.layers.set(HIDDEN_LAYER);
      tracked.push({ mesh, group, instance, chain });
    }
    this.parts.set(body, tracked);
  }

  remove(body: CrowdMario): void {
    for (const part of this.parts.get(body) ?? []) {
      part.group.batch.deleteInstance(part.instance);
      part.mesh.layers.set(RENDER_LAYER);
    }
    this.parts.delete(body);
  }

  /** One batch per (material, attribute layout); sized for `capacity` Marios. */
  private groupFor(mesh: THREE.Mesh, siblings: THREE.Mesh[]): BatchGroup | null {
    const material = mesh.material;
    if (Array.isArray(material)) return null;
    const key = this.keyOf(mesh);
    let group = this.groups.get(key);
    if (!group) {
      const geometries = [...new Set(siblings.filter(other => this.keyOf(other) === key).map(other => other.geometry))];
      const perBody = siblings.filter(other => this.keyOf(other) === key).length;
      const vertices = geometries.reduce((sum, g) => sum + g.getAttribute('position').count, 0);
      const indices = geometries.reduce((sum, g) => sum + (g.index?.count ?? 0), 0);
      const batchMaterial = (material as THREE.MeshStandardMaterial).clone();
      // Instance colours carry each part's colour (and team tints) instead.
      (batchMaterial as THREE.MeshStandardMaterial).color?.set(0xffffff);
      const batch = new THREE.BatchedMesh(perBody * this.capacity, Math.max(vertices, 3), Math.max(indices, 3), batchMaterial);
      batch.castShadow = false;
      batch.receiveShadow = false;
      batch.sortObjects = false;
      group = { batch, geometryIds: new Map() };
      for (const geometry of geometries) group.geometryIds.set(geometry, batch.addGeometry(geometry));
      this.groups.set(key, group);
      this.root.add(batch);
    }
    return group;
  }

  private keyOf(mesh: THREE.Mesh): string {
    const material = mesh.material as THREE.Material;
    const attributes = Object.keys(mesh.geometry.attributes).sort().join(',');
    // Tinted clones share a batch with their source material; the colour rides on the instance.
    const base = (mesh.userData.crowdBase as THREE.Material | undefined) ?? material;
    return `${base.uuid}|${attributes}|${mesh.geometry.index ? 'i' : 'n'}`;
  }

  /** Call after the rigs have animated and faced the camera this frame. */
  sync(): void {
    for (const [body, parts] of this.parts) {
      const shown = body.root.visible;
      body.root.updateMatrixWorld(true);
      for (const part of parts) {
        let visible = shown;
        for (const node of part.chain) if (!node.visible) { visible = false; break; }
        part.group.batch.setVisibleAt(part.instance, visible);
        if (!visible) continue;
        part.group.batch.setMatrixAt(part.instance, part.mesh.matrixWorld);
        const material = part.mesh.material as THREE.MeshStandardMaterial;
        part.group.batch.setColorAt(part.instance, this.color.copy(material.color));
      }
    }
  }

  dispose(): void {
    for (const group of this.groups.values()) {
      group.batch.dispose();
      (group.batch.material as THREE.Material).dispose();
    }
    this.groups.clear();
    this.parts.clear();
    this.root.removeFromParent();
  }
}
