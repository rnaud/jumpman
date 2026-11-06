import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { instantiateSm64, loadSm64Asset, SM64_MODELS } from './sm64Assets';
import type { InputManager } from './Input';

type ActorKind = 'mario' | 'goomba';
type DioramaActor = {
  kind: ActorKind;
  group: THREE.Group;
  pos: THREE.Vector3;
  target: THREE.Vector3;
  yaw: number;
  speed: number;
  timer: number;
  hop: number;
  hopVelocity: number;
  chaseTimer: number;
  baseScale: number;
};

const BOARD_TOP = 20;
const ARENA = 470;

function modelSize(group: THREE.Object3D): number {
  const box = new THREE.Box3().setFromObject(group);
  return Math.max(0.001, box.getSize(new THREE.Vector3()).y);
}

/**
 * Poly-pocket diorama: a miniature real-asset board, autonomous Marios and
 * Goombas, an FPS-style free-fly camera, and clickable actors.
 */
export class Diorama {
  enabled = false;
  readonly root = new THREE.Group();
  private readonly actors: DioramaActor[] = [];
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly selectedRing: THREE.Mesh;
  private cameraPosition = new THREE.Vector3(0, 1300, 3100);
  private yaw = Math.PI;
  private pitch = -0.38;
  private ready = false;
  private loading = false;
  private selected: DioramaActor | null = null;
  onNotify?: (message: string) => void;

  constructor(private readonly scene: THREE.Scene) {
    this.root.visible = false;
    this.root.name = 'diorama';
    scene.add(this.root);
    const ring = new THREE.Mesh(
      new THREE.RingGeometry(32, 42, 32),
      new THREE.MeshBasicMaterial({ color: 0xffd24a, side: THREE.DoubleSide, transparent: true, opacity: 0.9 }),
    );
    ring.rotation.x = -Math.PI / 2;
    ring.visible = false;
    this.selectedRing = ring;
    this.root.add(ring);
    this.buildBoard();
  }

  private buildBoard(): void {
    const base = new THREE.Mesh(
      new THREE.BoxGeometry(1250, 60, 1250),
      new THREE.MeshStandardMaterial({ color: 0x3e7d3a, roughness: 0.9 }),
    );
    base.position.y = -30;
    base.receiveShadow = true;
    this.root.add(base);
    const trim = new THREE.Mesh(
      new THREE.BoxGeometry(1320, 24, 1320),
      new THREE.MeshStandardMaterial({ color: 0x7c4b26, roughness: 0.95 }),
    );
    trim.position.y = -66;
    trim.castShadow = true;
    trim.receiveShadow = true;
    this.root.add(trim);
  }

  async enable(): Promise<void> {
    if (this.enabled) return;
    this.enabled = true;
    this.root.visible = true;
    this.cameraPosition.set(0, 1300, 3100);
    this.yaw = Math.PI;
    this.pitch = -0.38;
    if (this.loading || this.ready) return;
    this.loading = true;
    try {
      await this.loadBoard();
      this.ready = true;
    } catch (error) {
      console.error('[jumpman] diorama load failed', error);
    } finally {
      this.loading = false;
    }
  }

  private async loadBoard(): Promise<void> {
    const field = await this.createModel(SM64_MODELS.battlefield, 1150);
    this.fitToBoard(field.group, 1040);
    field.group.position.y = BOARD_TOP;
    this.root.add(field.group);
    const tree = await this.loadTemplate(SM64_MODELS.tree);
    const mario = await this.loadTemplate(SM64_MODELS.mario);
    const goomba = await this.loadTemplate(SM64_MODELS.goomba);
    for (let i = 0; i < 9; i++) {
      const angle = (i / 9) * Math.PI * 2;
      const x = Math.sin(angle) * (360 + (i % 3) * 45);
      const z = Math.cos(angle) * (360 + (i % 3) * 45);
      const model = this.normalizeModel(instantiateSm64(tree), 120);
      model.position.set(x, BOARD_TOP, z);
      this.root.add(model);
    }
    for (let i = 0; i < 12; i++) {
      const angle = (i / 12) * Math.PI * 2 + 0.35;
      const radius = 90 + (i % 4) * 95;
      const group = this.normalizeModel(instantiateSm64(mario), 100);
      group.traverse(node => { if (node instanceof THREE.Mesh) node.userData.dioramaActor = i; });
      const actor: DioramaActor = {
        kind: 'mario', group,
        pos: new THREE.Vector3(Math.sin(angle) * radius, BOARD_TOP, Math.cos(angle) * radius),
        target: new THREE.Vector3(Math.sin(angle) * 300, BOARD_TOP, Math.cos(angle) * 300),
        yaw: angle, speed: 28 + (i % 5) * 6, timer: Math.random() * 3,
        hop: 0, hopVelocity: 0, chaseTimer: 0,
        baseScale: group.userData.dioramaBaseScale,
      };
      this.root.add(group);
      this.actors.push(actor);
    }
    for (let i = 0; i < 6; i++) {
      const angle = (i / 6) * Math.PI * 2 + 0.8;
      const group = this.normalizeModel(instantiateSm64(goomba), 58);
      group.traverse(node => { if (node instanceof THREE.Mesh) node.userData.dioramaActor = 100 + i; });
      const actor: DioramaActor = {
        kind: 'goomba', group,
        pos: new THREE.Vector3(Math.sin(angle) * 280, BOARD_TOP, Math.cos(angle) * 280),
        target: new THREE.Vector3(Math.sin(angle) * 400, BOARD_TOP, Math.cos(angle) * 400),
        yaw: angle, speed: 16 + (i % 3) * 4, timer: Math.random() * 3,
        hop: 0, hopVelocity: 0, chaseTimer: 0,
        baseScale: group.userData.dioramaBaseScale,
      };
      this.root.add(group);
      this.actors.push(actor);
    }
    for (const actor of this.actors) this.placeActor(actor);
    this.onNotify?.('DIORAMA · CLICK A MARIO OR GOOMBA');
  }

  private async loadTemplate(path: string): Promise<GLTF> {
    return loadSm64Asset(path);
  }

  private async createModel(path: string, height: number): Promise<{ group: THREE.Group }> {
    return { group: this.normalizeModel(instantiateSm64(await this.loadTemplate(path)), height) };
  }

  private fitToBoard(group: THREE.Object3D, maxSize: number): void {
    const box = new THREE.Box3().setFromObject(group);
    const size = box.getSize(new THREE.Vector3());
    if (Math.max(size.x, size.z) < 1) return;
    group.scale.multiplyScalar(maxSize / Math.max(size.x, size.z));
    const fitted = new THREE.Box3().setFromObject(group);
    const center = fitted.getCenter(new THREE.Vector3());
    group.position.x -= center.x;
    group.position.z -= center.z;
    group.position.y -= fitted.min.y;
  }

  private normalizeModel(group: THREE.Group, height: number): THREE.Group {
    const current = modelSize(group);
    const scale = height / current;
    group.scale.multiplyScalar(scale);
    group.userData.dioramaBaseScale = group.scale.x;
    const box = new THREE.Box3().setFromObject(group);
    group.position.y -= box.min.y;
    group.traverse(node => {
      if (node instanceof THREE.Mesh) {
        node.castShadow = true;
        node.receiveShadow = true;
      }
    });
    return group;
  }

  disable(): void {
    this.enabled = false;
    this.root.visible = false;
    this.selected = null;
    this.selectedRing.visible = false;
  }

  update(dt: number, input: InputManager, camera: THREE.PerspectiveCamera): void {
    if (!this.enabled) return;
    const look = input.consumeCameraInput();
    this.yaw += look.dx * 0.0032;
    this.pitch = THREE.MathUtils.clamp(this.pitch - look.dy * 0.0026, -1.35, 1.35);
    const speed = input.isDown('ShiftLeft') || input.isDown('ShiftRight') ? 1100 : 520;
    const forward = new THREE.Vector3(Math.sin(this.yaw), 0, Math.cos(this.yaw));
    const right = new THREE.Vector3(Math.cos(this.yaw), 0, -Math.sin(this.yaw));
    if (input.isDown('KeyW')) this.cameraPosition.addScaledVector(forward, speed * dt);
    if (input.isDown('KeyS')) this.cameraPosition.addScaledVector(forward, -speed * dt);
    if (input.isDown('KeyD')) this.cameraPosition.addScaledVector(right, speed * dt);
    if (input.isDown('KeyA')) this.cameraPosition.addScaledVector(right, -speed * dt);
    if (input.isDown('KeyE') || input.isDown('Space')) this.cameraPosition.y += speed * dt;
    if (input.isDown('KeyQ')) this.cameraPosition.y -= speed * dt;
    this.cameraPosition.y = Math.max(120, this.cameraPosition.y);
    camera.position.copy(this.cameraPosition);
    const target = this.cameraPosition.clone().add(new THREE.Vector3(
      Math.sin(this.yaw) * Math.cos(this.pitch),
      Math.sin(this.pitch),
      Math.cos(this.yaw) * Math.cos(this.pitch),
    ).multiplyScalar(800));
    camera.up.set(0, 1, 0);
    camera.lookAt(target);
    for (const actor of this.actors) this.stepActor(actor, dt);
    this.selectedRing.position.set(this.selected?.pos.x ?? 0, BOARD_TOP + 3, this.selected?.pos.z ?? 0);
  }

  private stepActor(actor: DioramaActor, dt: number): void {
    actor.timer -= dt;
    if (actor.timer <= 0 || actor.pos.distanceTo(actor.target) < 24) {
      actor.timer = 1.4 + Math.random() * 3;
      const angle = Math.random() * Math.PI * 2;
      const radius = 80 + Math.random() * 340;
      actor.target.set(Math.sin(angle) * radius, BOARD_TOP, Math.cos(angle) * radius);
    }
    if (actor.chaseTimer > 0) {
      actor.chaseTimer -= dt;
      const nearest = this.actors
        .filter(candidate => candidate.kind === 'mario')
        .sort((a, b) => a.pos.distanceToSquared(actor.pos) - b.pos.distanceToSquared(actor.pos))[0];
      if (nearest) actor.target.set(nearest.pos.x, BOARD_TOP, nearest.pos.z);
    }
    const dx = actor.target.x - actor.pos.x;
    const dz = actor.target.z - actor.pos.z;
    if (Math.hypot(dx, dz) > 12) {
      actor.yaw = Math.atan2(dx, dz);
      const boost = actor.chaseTimer > 0 ? 1.8 : 1;
      actor.pos.x += Math.sin(actor.yaw) * actor.speed * boost * dt;
      actor.pos.z += Math.cos(actor.yaw) * actor.speed * boost * dt;
      actor.pos.x = THREE.MathUtils.clamp(actor.pos.x, -ARENA, ARENA);
      actor.pos.z = THREE.MathUtils.clamp(actor.pos.z, -ARENA, ARENA);
    }
    if (actor.hop > 0 || actor.hopVelocity !== 0) {
      actor.hopVelocity -= 620 * dt;
      actor.hop += actor.hopVelocity * dt;
      if (actor.hop <= 0) { actor.hop = 0; actor.hopVelocity = 0; }
    }
    this.placeActor(actor);
  }

  private placeActor(actor: DioramaActor): void {
    actor.group.position.set(actor.pos.x, BOARD_TOP + actor.hop, actor.pos.z);
    actor.group.rotation.y = actor.yaw;
    const bob = actor.kind === 'mario' ? Math.sin(performance.now() * 0.006 + actor.pos.x) * 2 : 0;
    actor.group.position.y += bob;
    const pulse = actor === this.selected ? 1 + Math.sin(performance.now() * 0.008) * 0.06 : 1;
    actor.group.scale.setScalar(actor.baseScale * pulse);
  }

  click(clientX: number, clientY: number, camera: THREE.PerspectiveCamera, canvas: HTMLCanvasElement): boolean {
    if (!this.enabled) return false;
    const rect = canvas.getBoundingClientRect();
    this.pointer.set(
      ((clientX - rect.left) / rect.width) * 2 - 1,
      -((clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(this.pointer, camera);
    const hit = this.raycaster.intersectObjects(this.actors.map(actor => actor.group), true)[0];
    if (!hit) return false;
    const actor = this.actors.find(candidate => {
      let found = false;
      candidate.group.traverse(node => { if (node === hit.object) found = true; });
      return found;
    });
    if (!actor) return false;
    this.trigger(actor);
    return true;
  }

  clickActor(index: number): boolean {
    const actor = this.actors[index];
    if (!this.enabled || !actor) return false;
    this.trigger(actor);
    return true;
  }

  private trigger(actor: DioramaActor): void {
    this.selected = actor;
    this.selectedRing.visible = true;
    if (actor.kind === 'mario') {
      actor.hopVelocity = 240 + Math.random() * 120;
      actor.hop = Math.max(actor.hop, 1);
      this.onNotify?.(`MARIO #${this.actors.indexOf(actor) + 1} · JUMP`);
    } else {
      actor.chaseTimer = 6;
      this.onNotify?.(`GOOMBA #${this.actors.filter(a => a.kind === 'goomba').indexOf(actor) + 1} · CHASE`);
    }
  }

  debug(): { enabled: boolean; ready: boolean; actors: number; selected: number; kinds: string[] } {
    return {
      enabled: this.enabled,
      ready: this.ready,
      actors: this.actors.length,
      selected: this.selected ? this.actors.indexOf(this.selected) : -1,
      kinds: this.actors.map(actor => actor.kind),
    };
  }

  dispose(): void {
    this.scene.remove(this.root);
  }
}
