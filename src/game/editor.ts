import * as THREE from 'three';
import type { CollisionWorld, Surface } from '../sim/collision';

export interface EditorBlockDebug {
  id: number;
  pos: [number, number, number];
  size: [number, number, number];
}

/**
 * Small runtime platform editor: click the terrain to spawn a platform, drag a
 * platform to move it, scroll to resize it, right-click/Delete to remove it.
 * Display geometry and collision surfaces stay synchronized.
 */
export class RuntimeEditor {
  enabled = false;
  selected: number | null = null;

  private readonly blocks = new Map<number, {
    mesh: THREE.Mesh;
    surfaces: Set<Surface>;
    size: THREE.Vector3;
    center: THREE.Vector3;
  }>();
  private nextId = 1;
  private storageKey: string | null = null;
  private undoStack: Array<Array<{ id: number; center: [number, number, number]; size: [number, number, number] }>> = [];
  private redoStack: typeof this.undoStack = [];
  private dragging: number | null = null;
  private dragOffset = new THREE.Vector3();
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly dragPlane = new THREE.Plane();
  private readonly hitPoint = new THREE.Vector3();
  private readonly blockGeometry = new THREE.BoxGeometry(1, 1, 1);
  private readonly blockMaterial = new THREE.MeshStandardMaterial({
    color: 0x55d6c2,
    roughness: 0.72,
    metalness: 0.08,
    transparent: true,
    opacity: 0.9,
  });

  constructor(
    private readonly scene: THREE.Scene,
    private readonly camera: THREE.PerspectiveCamera,
    private readonly canvas: HTMLCanvasElement,
    private terrain: THREE.Mesh[],
    private world: CollisionWorld,
    storageKey: string | null = null,
  ) {
    this.storageKey = storageKey;
    this.load();
  }

  setWorld(world: CollisionWorld, terrain: THREE.Mesh[], storageKey?: string): void {
    this.clear();
    this.world = world;
    this.terrain = terrain;
    this.storageKey = storageKey ?? null;
    this.load();
  }

  private setPointer(event: PointerEvent): void {
    const rect = this.canvas.getBoundingClientRect();
    this.pointer.set(
      ((event.clientX - rect.left) / rect.width) * 2 - 1,
      -((event.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(this.pointer, this.camera);
  }

  pointerDown(event: PointerEvent): boolean {
    if (!this.enabled) return false;
    this.setPointer(event);

    if (event.button === 2 && this.selected !== null) {
      this.remove(this.selected);
      return true;
    }

    const blockMeshes = [...this.blocks.values()].map(block => block.mesh);
    const blockHit = this.raycaster.intersectObjects(blockMeshes, false)[0];
    if (blockHit) {
      const id = Number(blockHit.object.userData.editorId);
      this.record();
      this.selected = id;
      this.dragging = id;
      const block = this.blocks.get(id)!;
      this.dragPlane.setFromNormalAndCoplanarPoint(new THREE.Vector3(0, 1, 0), block.center);
      if (this.raycaster.ray.intersectPlane(this.dragPlane, this.hitPoint)) {
        this.dragOffset.copy(block.center).sub(this.hitPoint);
      }
      return true;
    }

    const terrainHit = this.raycaster.intersectObjects(this.terrain, false)[0];
    if (terrainHit) {
      this.spawn(new THREE.Vector3(terrainHit.point.x, terrainHit.point.y, terrainHit.point.z));
      return true;
    }
    return false;
  }

  pointerMove(event: PointerEvent): boolean {
    if (!this.enabled || this.dragging === null) return false;
    this.setPointer(event);
    const block = this.blocks.get(this.dragging);
    if (!block) return true;
    this.dragPlane.setFromNormalAndCoplanarPoint(new THREE.Vector3(0, 1, 0), block.center);
    if (!this.raycaster.ray.intersectPlane(this.dragPlane, this.hitPoint)) return true;
    const next = this.hitPoint.clone().add(this.dragOffset);
    block.center.set(next.x, block.center.y, next.z);
    this.refresh(block);
    return true;
  }

  pointerUp(): void {
    if (this.dragging !== null) this.save();
    this.dragging = null;
  }

  wheel(delta: number): boolean {
    if (!this.enabled || this.selected === null) return false;
    const block = this.blocks.get(this.selected);
    if (!block) return false;
    this.record();
    const amount = Math.sign(delta) * 12;
    block.size.x = Math.max(60, block.size.x + amount);
    block.size.z = Math.max(60, block.size.z + amount);
    block.size.y = Math.max(20, block.size.y + amount * 0.25);
    this.refresh(block);
    this.save();
    return true;
  }

  deleteSelected(): boolean {
    if (this.selected === null) return false;
    this.remove(this.selected);
    return true;
  }

  spawn(at: THREE.Vector3): number {
    this.record();
    const id = this.nextId++;
    this.createBlock(id, new THREE.Vector3(at.x, at.y + 18, at.z), new THREE.Vector3(260, 36, 260));
    this.selected = id;
    this.save();
    return id;
  }

  remove(id: number): void {
    this.record();
    this.removeBlock(id);
    this.save();
  }

  reset(): void {
    this.clear();
    this.save();
  }

  undo(): boolean {
    const state = this.undoStack.pop();
    if (!state) return false;
    this.redoStack.push(this.snapshot());
    this.restore(state);
    this.save();
    return true;
  }

  redo(): boolean {
    const state = this.redoStack.pop();
    if (!state) return false;
    this.undoStack.push(this.snapshot());
    this.restore(state);
    this.save();
    return true;
  }

  clear(): void {
    for (const id of [...this.blocks.keys()]) this.removeBlock(id);
    this.selected = null;
    this.dragging = null;
    this.undoStack = [];
    this.redoStack = [];
  }

  private createBlock(id: number, center: THREE.Vector3, size: THREE.Vector3): void {
    const mesh = new THREE.Mesh(this.blockGeometry, this.blockMaterial);
    mesh.userData.editorId = id;
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    this.scene.add(mesh);
    const block = { mesh, surfaces: new Set<Surface>(), size, center };
    this.blocks.set(id, block);
    this.refresh(block);
  }

  private removeBlock(id: number): void {
    const block = this.blocks.get(id);
    if (!block) return;
    this.world.removeSurfaces(block.surfaces);
    this.scene.remove(block.mesh);
    this.blocks.delete(id);
    if (this.selected === id) this.selected = null;
    if (this.dragging === id) this.dragging = null;
  }

  private load(): void {
    if (!this.storageKey) return;
    try {
      const raw = localStorage.getItem(this.storageKey);
      if (!raw) return;
      const data = JSON.parse(raw) as Array<{ id: number; center: [number, number, number]; size: [number, number, number] }>;
      if (!Array.isArray(data)) return;
      for (const entry of data) {
        this.createBlock(entry.id, new THREE.Vector3(...entry.center), new THREE.Vector3(...entry.size));
        this.nextId = Math.max(this.nextId, entry.id + 1);
      }
      this.selected = data.length > 0 ? data[data.length - 1].id : null;
    } catch {
      // Ignore corrupt or unavailable storage.
    }
  }

  private save(): void {
    if (!this.storageKey) return;
    try {
      localStorage.setItem(this.storageKey, JSON.stringify(this.snapshot()));
    } catch {
      // Storage is optional.
    }
  }

  private snapshot(): Array<{ id: number; center: [number, number, number]; size: [number, number, number] }> {
    return [...this.blocks.entries()].map(([id, block]) => ({
      id,
      center: block.center.toArray() as [number, number, number],
      size: block.size.toArray() as [number, number, number],
    }));
  }

  private record(): void {
    this.undoStack.push(this.snapshot());
    if (this.undoStack.length > 50) this.undoStack.shift();
    this.redoStack = [];
  }

  private restore(state: Array<{ id: number; center: [number, number, number]; size: [number, number, number] }>): void {
    for (const id of [...this.blocks.keys()]) this.removeBlock(id);
    for (const entry of state) {
      this.createBlock(entry.id, new THREE.Vector3(...entry.center), new THREE.Vector3(...entry.size));
      this.nextId = Math.max(this.nextId, entry.id + 1);
    }
    this.selected = state.length > 0 ? state[state.length - 1].id : null;
    this.dragging = null;
  }

  dispose(): void {
    this.clear();
    this.blockGeometry.dispose();
    this.blockMaterial.dispose();
  }

  debug(): EditorBlockDebug[] {
    return [...this.blocks.entries()].map(([id, block]) => ({
      id,
      pos: block.center.toArray() as [number, number, number],
      size: block.size.toArray() as [number, number, number],
    }));
  }

  private refresh(block: { mesh: THREE.Mesh; surfaces: Set<Surface>; size: THREE.Vector3; center: THREE.Vector3 }): void {
    this.world.removeSurfaces(block.surfaces);
    block.surfaces.clear();
    block.mesh.position.copy(block.center);
    block.mesh.scale.copy(block.size);
    const { x, y, z } = block.center;
    const hx = block.size.x / 2;
    const hy = block.size.y / 2;
    const hz = block.size.z / 2;
    const top = y + hy;
    const add = (a: [number, number, number], b: [number, number, number], c: [number, number, number], d: [number, number, number]) => {
      for (const surface of this.world.addQuad(a, b, c, d)) block.surfaces.add(surface);
    };
    add([x - hx, top, z - hz], [x - hx, top, z + hz], [x + hx, top, z + hz], [x + hx, top, z - hz]);
    add([x - hx, y - hy, z - hz], [x + hx, y - hy, z - hz], [x + hx, y - hy, z + hz], [x - hx, y - hy, z + hz]);
    // Side walls keep Mario from walking through the platform volume.
    add([x - hx, top, z - hz], [x + hx, top, z - hz], [x + hx, y - hy, z - hz], [x - hx, y - hy, z - hz]);
    add([x + hx, top, z + hz], [x - hx, top, z + hz], [x - hx, y - hy, z + hz], [x + hx, y - hy, z + hz]);
    add([x - hx, top, z + hz], [x - hx, top, z - hz], [x - hx, y - hy, z - hz], [x - hx, y - hy, z + hz]);
    add([x + hx, top, z - hz], [x + hx, top, z + hz], [x + hx, y - hy, z + hz], [x + hx, y - hy, z - hz]);
  }
}
