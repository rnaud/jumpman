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
  ) {}

  setWorld(world: CollisionWorld, terrain: THREE.Mesh[]): void {
    this.clear();
    this.world = world;
    this.terrain = terrain;
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
    this.dragging = null;
  }

  wheel(delta: number): boolean {
    if (!this.enabled || this.selected === null) return false;
    const block = this.blocks.get(this.selected);
    if (!block) return false;
    const amount = Math.sign(delta) * 12;
    block.size.x = Math.max(60, block.size.x + amount);
    block.size.z = Math.max(60, block.size.z + amount);
    block.size.y = Math.max(20, block.size.y + amount * 0.25);
    this.refresh(block);
    return true;
  }

  deleteSelected(): boolean {
    if (this.selected === null) return false;
    this.remove(this.selected);
    return true;
  }

  spawn(at: THREE.Vector3): number {
    const id = this.nextId++;
    const size = new THREE.Vector3(260, 36, 260);
    const center = new THREE.Vector3(at.x, at.y + size.y / 2, at.z);
    const mesh = new THREE.Mesh(this.blockGeometry, this.blockMaterial);
    mesh.position.copy(center);
    mesh.scale.copy(size);
    mesh.userData.editorId = id;
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    this.scene.add(mesh);
    const block = { mesh, surfaces: new Set<Surface>(), size, center };
    this.blocks.set(id, block);
    this.refresh(block);
    this.selected = id;
    return id;
  }

  remove(id: number): void {
    const block = this.blocks.get(id);
    if (!block) return;
    this.world.removeSurfaces(block.surfaces);
    this.scene.remove(block.mesh);
    this.blocks.delete(id);
    if (this.selected === id) this.selected = null;
    if (this.dragging === id) this.dragging = null;
  }

  clear(): void {
    for (const id of [...this.blocks.keys()]) this.remove(id);
    this.selected = null;
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
  }
}
