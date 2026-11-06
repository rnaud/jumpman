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

interface PartBatch {
  mesh: THREE.InstancedMesh;
  /** Instances shown this frame (for culling and the draw count). */
  visible: number;
}

interface TrackedPart {
  mesh: THREE.Mesh;
  batch: PartBatch;
  slot: number;
  /** Ancestors up to the body root, checked each frame for switch-node visibility. */
  chain: THREE.Object3D[];
}

const RENDER_LAYER = 0;
const HIDDEN_LAYER = 7;
const HIDDEN_MATRIX = new THREE.Matrix4().makeScale(0, 0, 0);

/**
 * Draw a whole crowd with one draw call per Mario body part instead of one per
 * part per Mario: each part becomes an InstancedMesh with one instance per
 * Mario, fed the part's animated world matrix and its (possibly tinted) colour.
 * The original rigs keep animating but move to a layer the camera skips.
 */
export class CrowdBatcher {
  readonly root = new THREE.Group();
  private readonly batches = new Map<string, PartBatch>();
  private readonly parts = new Map<CrowdMario, TrackedPart[]>();
  private readonly free = new Map<PartBatch, number[]>();
  private readonly used = new Map<PartBatch, number>();
  private readonly color = new THREE.Color();

  constructor(private readonly capacity: number) {
    this.root.name = 'crowd-batches';
  }

  get drawCalls(): number {
    let calls = 0;
    for (const batch of this.batches.values()) if (batch.visible > 0) calls++;
    return calls;
  }

  add(body: CrowdMario): void {
    const tracked: TrackedPart[] = [];
    body.root.traverse(node => {
      if (!(node instanceof THREE.Mesh) || Array.isArray(node.material)) return;
      const batch = this.batchFor(node);
      const slot = this.free.get(batch)?.pop() ?? this.nextSlot(batch);
      if (slot < 0) return;
      const chain: THREE.Object3D[] = [];
      for (let n: THREE.Object3D | null = node; n && n !== body.root; n = n.parent) chain.push(n);
      node.layers.set(HIDDEN_LAYER);
      tracked.push({ mesh: node, batch, slot, chain });
    });
    this.parts.set(body, tracked);
  }

  remove(body: CrowdMario): void {
    for (const part of this.parts.get(body) ?? []) {
      part.batch.mesh.setMatrixAt(part.slot, HIDDEN_MATRIX);
      part.batch.mesh.instanceMatrix.needsUpdate = true;
      let list = this.free.get(part.batch);
      if (!list) this.free.set(part.batch, list = []);
      list.push(part.slot);
      part.mesh.layers.set(RENDER_LAYER);
    }
    this.parts.delete(body);
  }

  private nextSlot(batch: PartBatch): number {
    const next = this.used.get(batch) ?? 0;
    if (next >= this.capacity) return -1;
    this.used.set(batch, next + 1);
    batch.mesh.count = Math.max(batch.mesh.count, next + 1);
    return next;
  }

  /** One instanced mesh per (geometry, base material): every Mario shares the same parts. */
  private batchFor(mesh: THREE.Mesh): PartBatch {
    const base = (mesh.userData.crowdBase as THREE.Material | undefined) ?? (mesh.material as THREE.Material);
    const key = `${mesh.geometry.uuid}|${base.uuid}`;
    let batch = this.batches.get(key);
    if (!batch) {
      const material = (base as THREE.MeshStandardMaterial).clone();
      // Instance colours carry each part's colour (and team tints) instead.
      material.color?.set(0xffffff);
      const instanced = new THREE.InstancedMesh(mesh.geometry, material, this.capacity);
      instanced.count = 0;
      instanced.castShadow = false;
      instanced.receiveShadow = false;
      instanced.frustumCulled = false;
      instanced.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
      instanced.setColorAt(0, this.color.set(0xffffff));
      batch = { mesh: instanced, visible: 0 };
      this.batches.set(key, batch);
      this.root.add(instanced);
    }
    return batch;
  }

  /** Call after the rigs have animated and faced the camera this frame. */
  sync(): void {
    for (const batch of this.batches.values()) batch.visible = 0;
    for (const [body, parts] of this.parts) {
      const shown = body.root.visible;
      body.root.updateMatrixWorld(true);
      for (const part of parts) {
        let visible = shown;
        for (const node of part.chain) if (!node.visible) { visible = false; break; }
        const instanced = part.batch.mesh;
        if (!visible) {
          instanced.setMatrixAt(part.slot, HIDDEN_MATRIX);
          continue;
        }
        part.batch.visible++;
        instanced.setMatrixAt(part.slot, part.mesh.matrixWorld);
        instanced.setColorAt(part.slot, this.color.copy((part.mesh.material as THREE.MeshStandardMaterial).color));
      }
    }
    for (const batch of this.batches.values()) {
      // Parts no Mario is showing right now (eye blinks, cap-in-hand) cost nothing.
      batch.mesh.visible = batch.visible > 0;
      batch.mesh.instanceMatrix.needsUpdate = true;
      if (batch.mesh.instanceColor) batch.mesh.instanceColor.needsUpdate = true;
    }
  }

  dispose(): void {
    for (const batch of this.batches.values()) {
      batch.mesh.dispose();
      (batch.mesh.material as THREE.Material).dispose();
    }
    this.batches.clear();
    this.parts.clear();
    this.root.removeFromParent();
  }
}
