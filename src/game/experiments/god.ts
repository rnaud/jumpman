import * as THREE from 'three';
import type { Surface } from '../../sim/collision';
import { coss, sins } from '../../sim/math';
import { ACT_FLAG_AIR, ACT_FREEFALL, ACT_TRIPLE_JUMP } from '../../sim/state';
import { atan2s } from '../../sim/math';
import type { EditorActorTarget } from '../courseObjects';
import type { PointerHandler } from '../Input';
import type { Coin } from '../world';
import { addBoxCollision, addRampCollision, rampGeometry, sm64Texture, tiledBox, type TextureName } from './geometry';
import type { Experiment, ExperimentHost } from './types';
import { actionButton, dock, el, segmented, titleCard, type Segmented } from './ui';

export type GodTool = 'grab' | 'bridge' | 'block' | 'ramp' | 'spring' | 'bobomb' | 'coin' | 'portal';
type PieceKind = 'plank' | 'block' | 'ramp' | 'spring';

interface PieceSpec { kind: PieceKind; center: [number, number, number]; size: [number, number, number]; yaw: number }
interface Piece {
  id: number;
  kind: PieceKind;
  mesh: THREE.Mesh;
  center: THREE.Vector3;
  size: THREE.Vector3;
  yaw: number;
  surfaces: Set<Surface>;
  pop: number;
}

interface Grabbable {
  id: string;
  object: THREE.Object3D;
  position: THREE.Vector3;
  move(position: THREE.Vector3): void;
  scale(factor: number): void;
  remove(): void;
  piece?: Piece;
}

const TOOLS: ReadonlyArray<{ value: GodTool; label: string; key: string; title: string }> = [
  { value: 'grab', label: 'Grab', key: '1', title: 'Drag anything · Shift+drag lifts · wheel resizes · right-click deletes' },
  { value: 'bridge', label: 'Bridge', key: '2', title: 'Drag to paint planks at Mario’s height' },
  { value: 'block', label: 'Block', key: '3', title: 'Click to drop a stone block' },
  { value: 'ramp', label: 'Ramp', key: '4', title: 'Click to place a ramp facing away from Mario' },
  { value: 'spring', label: 'Spring', key: '5', title: 'Click to place a spring pad' },
  { value: 'bobomb', label: 'Bob-omb', key: '6', title: 'Click to spawn a Bob-omb' },
  { value: 'coin', label: 'Coin', key: '7', title: 'Click to spawn a coin' },
  { value: 'portal', label: 'Portal', key: '8', title: 'Click any wall or floor: two linked portals that keep Mario’s speed' },
];
const GRAVITY = [
  { value: '0.35', label: 'Moon' },
  { value: '0.7', label: 'Floaty' },
  { value: '1', label: 'Normal' },
  { value: '1.8', label: 'Heavy' },
] as const;

const MATERIAL_FOR: Record<PieceKind, { texture: TextureName | null; color: number; emissive?: number }> = {
  plank: { texture: 'planks', color: 0xffffff },
  block: { texture: 'flagstone', color: 0xffffff },
  ramp: { texture: 'sandstone', color: 0xffffff },
  spring: { texture: null, color: 0xff6a2a, emissive: 0x5a1a00 },
};

/**
 * Mario vs. God: play normally while the mouse reshapes the world around him.
 * Every piece is real collision, rebuilt the moment it moves.
 */
export class GodModeExperiment implements Experiment {
  readonly id = 'god';
  readonly title = 'Mario vs. God';
  readonly course = 'bob' as const;
  private host!: ExperimentHost;
  tool: GodTool = 'grab';
  private readonly pieces = new Map<number, Piece>();
  private nextId = 1;
  private readonly materials = new Map<PieceKind, THREE.MeshStandardMaterial>();
  private undoStack: PieceSpec[][] = [];
  private redoStack: PieceSpec[][] = [];
  private readonly raycaster = new THREE.Raycaster();
  private readonly ndc = new THREE.Vector2();
  private drag: { target: Grabbable; plane: THREE.Plane; offset: THREE.Vector3; vertical: boolean; last: THREE.Vector3 } | null = null;
  private stroke: { plane: THREE.Plane; last: THREE.Vector3 | null; count: number } | null = null;
  private hovered: Grabbable | null = null;
  private readonly highlight: THREE.LineSegments;
  private readonly guide: THREE.Line;
  private readonly ghost: THREE.Mesh;
  private toolSeg!: Segmented<GodTool>;
  private gravitySeg!: Segmented<string>;
  private slowMo = false;
  private bounces = 0;
  private spawnedCoins: Coin[] = [];
  /** Two linked portals; the next placement replaces the older one. */
  private readonly portals: Array<{ center: THREE.Vector3; normal: THREE.Vector3; mesh: THREE.Group } | null> = [null, null];
  private nextPortal = 0;
  private portalCooldown = 0;
  private warps = 0;
  private pointerPos: { x: number; y: number } | null = null;

  constructor() {
    this.highlight = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(1, 1, 1)),
      new THREE.LineBasicMaterial({ color: 0xffd24a, depthTest: false, transparent: true }),
    );
    this.highlight.renderOrder = 10;
    this.highlight.visible = false;
    this.guide = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(), new THREE.Vector3(0, -1, 0)]),
      new THREE.LineDashedMaterial({ color: 0xffffff, dashSize: 30, gapSize: 20, depthTest: false, transparent: true, opacity: 0.8 }),
    );
    this.guide.renderOrder = 10;
    this.guide.visible = false;
    this.ghost = new THREE.Mesh(
      new THREE.CircleGeometry(1, 32),
      new THREE.MeshBasicMaterial({ color: 0xffd24a, transparent: true, opacity: 0.35, depthWrite: false }),
    );
    this.ghost.rotation.x = -Math.PI / 2;
    this.ghost.visible = false;
  }

  get timeScale(): number { return this.slowMo ? 0.35 : 1; }

  readonly pointer: PointerHandler = {
    down: event => this.pointerDown(event),
    move: event => this.pointerMove(event),
    up: () => this.pointerUp(),
    wheel: delta => this.wheel(delta),
  };

  enter(host: ExperimentHost): void {
    this.host = host;
    host.scene.add(this.highlight, this.guide, this.ghost);
    host.canvas.style.cursor = 'crosshair';
    titleCard(host.ui, '02', 'Mario vs. God', 'Keep playing Mario. Your mouse moves the world: drag his platforms away, paint a bridge under him.', [
      ['1–8', 'pick a tool'], ['Drag', 'move · Shift lifts'], ['Wheel', 'resize what you hover'], ['Right-click', 'delete'], ['T', 'slow motion'], ['⌘/Ctrl Z', 'undo'],
    ]);
    const bar = dock(host.ui);
    this.toolSeg = segmented(TOOLS, this.tool, tool => this.setTool(tool));
    this.gravitySeg = segmented(GRAVITY, '1', value => this.setGravity(Number(value)));
    const group = (label: string, node: HTMLElement) => {
      const wrap = el('div', 'xp-group');
      wrap.append(el('span', 'xp-group-label', label), node);
      return wrap;
    };
    const time = actionButton('Slow-mo', () => this.toggleSlowMo(), 'T');
    time.classList.add('xp-toggle');
    bar.append(
      group('Tools', this.toolSeg.element),
      group('Gravity', this.gravitySeg.element),
      group('Time', time),
      group('History', (() => {
        const row = el('div', 'xp-seg');
        row.append(actionButton('Undo', () => this.undo()), actionButton('Reset', () => this.reset()));
        return row;
      })()),
    );
    this.reset(false);
    host.toast('DRAG THE FLOATING BLOCKS · PRESS 2 TO PAINT A BRIDGE', 3600);
  }

  exit(): void {
    for (const id of [...this.pieces.keys()]) this.removePiece(id);
    for (const coin of this.spawnedCoins) {
      coin.group.removeFromParent();
      const list = this.host.playground.coins;
      const index = list.indexOf(coin);
      if (index >= 0) list.splice(index, 1);
    }
    this.spawnedCoins = [];
    this.host.mario.gravityScale = 1;
    this.highlight.removeFromParent();
    this.guide.removeFromParent();
    this.ghost.removeFromParent();
    for (const portal of this.portals) portal?.mesh.removeFromParent();
    for (const material of this.materials.values()) material.dispose();
    this.materials.clear();
    this.host.canvas.style.cursor = 'grab';
  }

  private setTool(tool: GodTool): void {
    this.tool = tool;
    this.toolSeg.set(tool);
    this.host.canvas.style.cursor = tool === 'grab' ? 'grab' : 'crosshair';
    this.host.toast(TOOLS.find(entry => entry.value === tool)!.title.toUpperCase(), 1600);
  }

  private setGravity(scale: number): void {
    this.host.mario.gravityScale = scale;
    this.gravitySeg.set(String(scale));
    this.host.toast(`GRAVITY ×${scale}`, 1200);
  }

  private toggleSlowMo(): void {
    this.slowMo = !this.slowMo;
    this.host.ui.querySelector('.xp-toggle')?.toggleAttribute('aria-pressed', this.slowMo);
    this.host.toast(this.slowMo ? 'SLOW MOTION' : 'NORMAL SPEED', 1000);
  }

  keydown(event: KeyboardEvent): boolean {
    const index = ['Digit1', 'Digit2', 'Digit3', 'Digit4', 'Digit5', 'Digit6', 'Digit7', 'Digit8'].indexOf(event.code);
    if (index >= 0) { this.setTool(TOOLS[index].value); return true; }
    if ((event.ctrlKey || event.metaKey) && event.code === 'KeyZ') {
      event.preventDefault();
      if (event.shiftKey) this.redo(); else this.undo();
      return true;
    }
    if (event.code === 'KeyT') { this.toggleSlowMo(); return true; }
    if (event.code === 'BracketLeft' || event.code === 'BracketRight') {
      const values = GRAVITY.map(entry => Number(entry.value));
      const current = values.indexOf(this.host.mario.gravityScale);
      const next = Math.max(0, Math.min(values.length - 1, (current < 0 ? 2 : current) + (event.code === 'BracketRight' ? 1 : -1)));
      this.setGravity(values[next]);
      return true;
    }
    if ((event.code === 'Delete' || event.code === 'Backspace') && this.hovered) {
      this.deleteTarget(this.hovered);
      return true;
    }
    return false;
  }

  // ---------------------------------------------------------------- pieces

  private material(kind: PieceKind): THREE.MeshStandardMaterial {
    let material = this.materials.get(kind);
    if (!material) {
      const spec = MATERIAL_FOR[kind];
      material = new THREE.MeshStandardMaterial({
        map: spec.texture ? sm64Texture(spec.texture) : null, color: spec.color, emissive: spec.emissive ?? 0x000000,
        roughness: 0.85, side: kind === 'ramp' ? THREE.DoubleSide : THREE.FrontSide,
      });
      this.materials.set(kind, material);
    }
    return material;
  }

  private createPiece(spec: PieceSpec, pop = true): Piece {
    const size = new THREE.Vector3(...spec.size);
    const geometry = spec.kind === 'ramp'
      ? rampGeometry(size.x, size.y, size.z, 200)
      : tiledBox(size.x, size.y, size.z, spec.kind === 'plank' ? 110 : 200);
    const mesh = new THREE.Mesh(geometry, this.material(spec.kind));
    mesh.castShadow = true;
    mesh.receiveShadow = true;
    const piece: Piece = {
      id: this.nextId++, kind: spec.kind, mesh, center: new THREE.Vector3(...spec.center), size, yaw: spec.yaw,
      surfaces: new Set(), pop: pop ? 0 : 1,
    };
    mesh.userData.godPiece = piece.id;
    this.host.scene.add(mesh);
    this.pieces.set(piece.id, piece);
    this.rebuild(piece);
    return piece;
  }

  private rebuild(piece: Piece): void {
    const world = this.host.playground.collision;
    world.removeSurfaces(piece.surfaces);
    piece.surfaces.clear();
    if (piece.kind === 'ramp') addRampCollision(world, piece.center, piece.size, piece.yaw, piece.surfaces);
    else addBoxCollision(world, piece.center, piece.size, piece.yaw, piece.surfaces);
    piece.mesh.position.copy(piece.center);
    piece.mesh.rotation.set(0, piece.yaw, 0);
  }

  private resizePiece(piece: Piece, factor: number): void {
    const next = piece.size.clone().multiplyScalar(factor);
    if (Math.max(next.x, next.z) > 2400 || Math.min(next.x, next.z) < 60) return;
    if (piece.kind === 'plank' || piece.kind === 'spring') next.y = piece.size.y;
    piece.size.copy(next);
    piece.mesh.geometry.dispose();
    piece.mesh.geometry = piece.kind === 'ramp' ? rampGeometry(next.x, next.y, next.z, 200) : tiledBox(next.x, next.y, next.z, piece.kind === 'plank' ? 110 : 200);
    this.rebuild(piece);
  }

  private removePiece(id: number): void {
    const piece = this.pieces.get(id);
    if (!piece) return;
    this.host.playground.collision.removeSurfaces(piece.surfaces);
    piece.mesh.removeFromParent();
    piece.mesh.geometry.dispose();
    this.pieces.delete(id);
    if (this.hovered?.piece === piece) this.hovered = null;
  }

  private snapshot(): PieceSpec[] {
    return [...this.pieces.values()].map(piece => ({
      kind: piece.kind, center: piece.center.toArray() as [number, number, number], size: piece.size.toArray() as [number, number, number], yaw: piece.yaw,
    }));
  }

  private record(): void {
    this.undoStack.push(this.snapshot());
    if (this.undoStack.length > 60) this.undoStack.shift();
    this.redoStack = [];
  }

  private restore(state: PieceSpec[]): void {
    for (const id of [...this.pieces.keys()]) this.removePiece(id);
    for (const spec of state) this.createPiece(spec, false);
  }

  undo(): void {
    const state = this.undoStack.pop();
    if (!state) { this.host.toast('NOTHING TO UNDO', 900); return; }
    this.redoStack.push(this.snapshot());
    this.restore(state);
    this.host.toast('UNDO', 700);
  }

  redo(): void {
    const state = this.redoStack.pop();
    if (!state) return;
    this.undoStack.push(this.snapshot());
    this.restore(state);
  }

  /** Clear everything and lay out a short floating route ahead of Mario. */
  reset(announce = true): void {
    if (announce) this.record();
    for (const id of [...this.pieces.keys()]) this.removePiece(id);
    const m = this.host.mario;
    const yaw = m.faceAngle[1];
    const fx = sins(yaw), fz = coss(yaw);
    const base = m.floorHeight;
    // A zig-zag staircase in the air, so the toy is obvious from the first frame.
    const steps: Array<[number, number, number, number]> = [[520, 170, 260, -220], [880, 330, 240, 220], [1240, 490, 240, -220], [1700, 650, 420, 0]];
    for (const [ahead, height, width, side] of steps) {
      this.createPiece({ kind: 'block', center: [m.pos[0] + fx * ahead + fz * side, base + height, m.pos[2] + fz * ahead - fx * side], size: [width, 60, width], yaw: Math.atan2(fx, fz) });
    }
    this.setGravity(1);
    if (announce) this.host.toast('WORLD RESET', 1000);
  }

  // ------------------------------------------------------------- targets

  private grabbables(): Grabbable[] {
    const list: Grabbable[] = [];
    for (const piece of this.pieces.values()) {
      list.push({
        id: `piece-${piece.id}`, object: piece.mesh, position: piece.center, piece,
        move: position => this.movePiece(piece, position),
        scale: factor => this.resizePiece(piece, factor),
        remove: () => { this.record(); this.removePiece(piece.id); },
      });
    }
    const actors: EditorActorTarget[] = this.host.playground.objects?.editorActors() ?? [];
    for (const actor of actors) {
      list.push({
        id: actor.id, object: actor.mesh, position: actor.position,
        move: position => actor.move(position),
        scale: factor => actor.setScale(Math.max(0.3, Math.min(4, actor.scale * factor))),
        remove: () => actor.remove(),
      });
    }
    (this.host.playground.goombas ?? []).forEach((enemy, index) => {
      if (!enemy.tangible) return;
      const model = enemy.model;
      list.push({
        id: `goomba-${index}`, object: model, position: model.position,
        move: position => {
          const floor = this.host.playground.collision.findFloor(position.x, position.y + 200, position.z);
          model.position.set(position.x, floor.surface ? Math.max(floor.height, position.y) : position.y, position.z);
          (enemy as unknown as { home?: THREE.Vector3 }).home?.copy(model.position);
        },
        scale: factor => model.scale.multiplyScalar(Math.max(0.3 / model.scale.x, Math.min(factor, 4 / model.scale.x))),
        remove: () => enemy.defeat(false),
      });
    });
    this.host.playground.coins.forEach((coin, index) => {
      if (coin.collected) return;
      list.push({
        id: `coin-${index}`, object: coin.group, position: coin.position,
        move: position => { coin.position.copy(position); coin.group.position.copy(position); },
        scale: factor => coin.group.scale.multiplyScalar(factor),
        remove: () => { coin.collected = true; coin.group.visible = false; },
      });
    });
    return list;
  }

  private ray(event: { clientX: number; clientY: number }): THREE.Raycaster {
    const rect = this.host.canvas.getBoundingClientRect();
    this.ndc.set(((event.clientX - rect.left) / rect.width) * 2 - 1, -((event.clientY - rect.top) / rect.height) * 2 + 1);
    this.raycaster.setFromCamera(this.ndc, this.host.camera);
    return this.raycaster;
  }

  private pickTarget(event: { clientX: number; clientY: number }): Grabbable | null {
    const ray = this.ray(event);
    const targets = this.grabbables();
    const hit = ray.intersectObjects(targets.map(target => target.object), true)[0];
    if (!hit) return null;
    return targets.find(target => {
      let node: THREE.Object3D | null = hit.object;
      while (node) { if (node === target.object) return true; node = node.parent; }
      return false;
    }) ?? null;
  }

  private terrainHit(event: { clientX: number; clientY: number }): THREE.Vector3 | null {
    const ray = this.ray(event);
    const meshes = [...this.host.playground.collisionMeshes, ...[...this.pieces.values()].map(piece => piece.mesh)];
    const hit = ray.intersectObjects(meshes, false)[0] ?? ray.intersectObject(this.host.courseScene, true)[0];
    return hit ? hit.point.clone() : null;
  }

  private movePiece(piece: Piece, position: THREE.Vector3): void {
    const delta = position.clone().sub(piece.center);
    if (delta.lengthSq() < 0.01) return;
    const m = this.host.mario;
    // A platform carries Mario when it moves under his feet.
    const riding = m.floor !== null && piece.surfaces.has(m.floor) && !(m.action & ACT_FLAG_AIR) && m.pos[1] <= m.floorHeight + 4;
    piece.center.copy(position);
    this.rebuild(piece);
    if (riding) {
      m.pos[0] += delta.x;
      m.pos[1] += delta.y;
      m.pos[2] += delta.z;
    }
  }

  private deleteTarget(target: Grabbable): void {
    this.host.particles.burst(target.object.getWorldPosition(new THREE.Vector3()), { count: 14, color: 0xffe8a0, speed: 160 });
    this.host.sfx.breakBox(target.position.toArray());
    target.remove();
    if (this.hovered === target) this.hovered = null;
  }

  // ------------------------------------------------------------- pointer

  private pointerDown(event: PointerEvent): boolean {
    this.pointerPos = { x: event.clientX, y: event.clientY };
    const target = this.pickTarget(event);
    if (event.button === 2) {
      if (target) { this.deleteTarget(target); return true; }
      return false;
    }
    if (event.button !== 0) return false;
    if (this.tool === 'grab') {
      if (!target) return false;
      if (target.piece) this.record();
      const vertical = event.shiftKey;
      const plane = new THREE.Plane();
      if (vertical) {
        const normal = new THREE.Vector3();
        this.host.camera.getWorldDirection(normal);
        normal.y = 0;
        normal.normalize().negate();
        plane.setFromNormalAndCoplanarPoint(normal, target.position);
      } else plane.setFromNormalAndCoplanarPoint(new THREE.Vector3(0, 1, 0), target.position);
      const hit = new THREE.Vector3();
      const offset = new THREE.Vector3();
      if (this.ray(event).ray.intersectPlane(plane, hit)) offset.copy(target.position).sub(hit);
      this.drag = { target, plane, offset, vertical, last: target.position.clone() };
      this.host.canvas.style.cursor = 'grabbing';
      return true;
    }
    if (this.tool === 'bridge') {
      this.record();
      const m = this.host.mario;
      // Planks appear at Mario's feet height, so you can catch him mid-fall.
      // Plank tops sit flush with his feet (or just under him while airborne).
      const height = m.action & ACT_FLAG_AIR ? m.pos[1] - 40 : m.floorHeight;
      this.stroke = { plane: new THREE.Plane(new THREE.Vector3(0, 1, 0), -height), last: null, count: 0 };
      this.extendStroke(event);
      return true;
    }
    this.place(event);
    return true;
  }

  private pointerMove(event: PointerEvent): boolean {
    this.pointerPos = { x: event.clientX, y: event.clientY };
    if (this.drag) {
      const hit = new THREE.Vector3();
      if (!this.ray(event).ray.intersectPlane(this.drag.plane, hit)) return true;
      const next = hit.add(this.drag.offset);
      if (this.drag.vertical) next.set(this.drag.last.x, next.y, this.drag.last.z);
      else next.y = this.drag.last.y;
      this.drag.target.move(next);
      this.drag.last.copy(next);
      return true;
    }
    if (this.stroke) {
      this.extendStroke(event);
      return true;
    }
    return false;
  }

  private pointerUp(): void {
    if (this.drag) this.host.canvas.style.cursor = 'grab';
    if (this.stroke && this.stroke.count === 0) this.undoStack.pop();
    this.drag = null;
    this.stroke = null;
  }

  private wheel(delta: number): boolean {
    const target = this.hovered;
    if (!target) return false;
    if (target.piece) this.record();
    target.scale(delta < 0 ? 1.12 : 1 / 1.12);
    return true;
  }

  private extendStroke(event: PointerEvent): void {
    const stroke = this.stroke!;
    const point = new THREE.Vector3();
    if (!this.ray(event).ray.intersectPlane(stroke.plane, point)) return;
    if (!stroke.last) { stroke.last = point; return; }
    const step = 90;
    let distance = point.distanceTo(stroke.last);
    while (distance >= step) {
      const direction = point.clone().sub(stroke.last).normalize();
      const center = stroke.last.clone().addScaledVector(direction, step / 2);
      const yaw = Math.atan2(direction.x, direction.z);
      this.createPiece({ kind: 'plank', center: [center.x, center.y - 15, center.z], size: [280, 30, 104], yaw });
      stroke.last.addScaledVector(direction, step);
      stroke.count++;
      distance -= step;
    }
    if (stroke.count % 4 === 1) this.host.sfx.footstep(false);
  }

  /** Place a portal flush on the clicked surface, facing out of it. */
  private placePortal(event: PointerEvent): void {
    const ray = this.ray(event);
    const meshes = [...this.host.playground.collisionMeshes, ...[...this.pieces.values()].map(piece => piece.mesh)];
    const hit = ray.intersectObjects(meshes, false)[0] ?? ray.intersectObject(this.host.courseScene, true)[0];
    if (!hit?.face) { this.host.toast('AIM AT A WALL OR FLOOR', 1000); return; }
    const normal = hit.face.normal.clone().applyMatrix3(new THREE.Matrix3().getNormalMatrix(hit.object.matrixWorld)).normalize();
    // Face the portal toward the camera if the mesh is single-sided the other way.
    if (normal.dot(ray.ray.direction) > 0) normal.negate();
    const slot = this.nextPortal;
    this.nextPortal = 1 - slot;
    this.portals[slot]?.mesh.removeFromParent();
    const color = slot === 0 ? 0x3aa8ff : 0xff8a1f;
    const mesh = new THREE.Group();
    const ring = new THREE.Mesh(new THREE.TorusGeometry(120, 16, 12, 40), new THREE.MeshBasicMaterial({ color }));
    const disc = new THREE.Mesh(new THREE.CircleGeometry(112, 40), new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.45, side: THREE.DoubleSide, depthWrite: false }));
    mesh.add(ring, disc);
    const center = hit.point.clone().addScaledVector(normal, 4);
    mesh.position.copy(center);
    mesh.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), normal);
    mesh.renderOrder = 4;
    this.host.scene.add(mesh);
    this.portals[slot] = { center, normal, mesh };
    this.host.particles.sparks(center, 14);
    this.host.toast(slot === 0 ? 'BLUE PORTAL' : 'ORANGE PORTAL', 900);
  }

  /** Enter one portal, leave the other at the same speed, flung out along its face. */
  private stepPortals(): void {
    if (this.portalCooldown > 0) { this.portalCooldown--; return; }
    const [a, b] = this.portals;
    if (!a || !b) return;
    const m = this.host.mario;
    const body = new THREE.Vector3(m.pos[0], m.pos[1] + 80, m.pos[2]);
    for (const [from, to] of [[a, b], [b, a]] as const) {
      const offset = body.clone().sub(from.center);
      const depth = offset.dot(from.normal);
      const lateral = offset.addScaledVector(from.normal, -depth).length();
      if (lateral > 115 || depth > 110 || depth < -60) continue;
      const yaw = m.faceAngle[1] * Math.PI / 32768;
      const velocity = new THREE.Vector3(Math.sin(yaw) * m.forwardVel, m.vel[1], Math.cos(yaw) * m.forwardVel);
      const speed = Math.max(28, velocity.length());
      // Moving away from the portal face? Then it isn't an entry.
      if (velocity.dot(from.normal) > 4 && depth > 40) continue;
      const out = to.normal;
      const exit = to.center.clone().addScaledVector(out, 140);
      m.pos = [exit.x, exit.y - 80, exit.z];
      m.setAction(ACT_FREEFALL, 0);
      const horizontal = Math.hypot(out.x, out.z);
      if (horizontal > 0.3) m.faceAngle[1] = atan2s(out.z, out.x);
      m.forwardVel = speed * horizontal;
      m.vel[1] = out.y * speed + (horizontal > 0.3 ? 12 : 0);
      this.portalCooldown = 18;
      this.warps++;
      this.host.particles.sparks(exit, 16);
      this.host.sfx.teleport();
      return;
    }
  }

  private place(event: PointerEvent): void {
    if (this.tool === 'portal') { this.placePortal(event); return; }
    const point = this.terrainHit(event);
    const m = this.host.mario;
    if (!point) { this.host.toast('CLICK THE GROUND OR A PLATFORM', 1000); return; }
    const facing = Math.atan2(point.x - m.pos[0], point.z - m.pos[2]);
    switch (this.tool) {
      case 'block':
        this.record();
        this.createPiece({ kind: 'block', center: [point.x, point.y + 120, point.z], size: [240, 240, 240], yaw: facing });
        break;
      case 'ramp':
        this.record();
        // Rises away from Mario (local -Z points away).
        this.createPiece({ kind: 'ramp', center: [point.x, point.y + 80, point.z], size: [320, 160, 520], yaw: facing + Math.PI });
        break;
      case 'spring':
        this.record();
        this.createPiece({ kind: 'spring', center: [point.x, point.y + 22, point.z], size: [170, 44, 170], yaw: 0 });
        break;
      case 'bobomb': {
        const objects = this.host.playground.objects;
        if (!objects) { this.host.toast('NO OBJECTS IN THIS COURSE', 1200); return; }
        objects.manager.spawn({ behavior: 'bobomb', model: 'actors/bobomb/black_bobomb_geo.glb', pos: [point.x, point.y + 60, point.z], yaw: 0 });
        break;
      }
      case 'coin': {
        const template = this.host.playground.coins[0]?.group;
        if (!template) { this.host.toast('NO COIN MODEL IN THIS COURSE', 1200); return; }
        const group = template.clone(true) as THREE.Group;
        group.visible = true;
        const position = new THREE.Vector3(point.x, point.y + 80, point.z);
        group.position.copy(position);
        (template.parent ?? this.host.courseScene).add(group);
        const coin: Coin = { group, position, collected: false };
        this.host.playground.coins.push(coin);
        this.spawnedCoins.push(coin);
        break;
      }
      default:
        return;
    }
    this.host.particles.burst(point, { count: 10, color: 0xfff1c0, speed: 120 });
  }

  // --------------------------------------------------------------- frame

  afterStep(): void {
    this.stepPortals();
    const m = this.host.mario;
    if (m.vel[1] > 0 || !m.floor || m.pos[1] > m.floorHeight + 4) return;
    for (const piece of this.pieces.values()) {
      if (piece.kind !== 'spring' || !piece.surfaces.has(m.floor)) continue;
      m.setAction(ACT_TRIPLE_JUMP, 0);
      m.vel[1] = 96;
      piece.pop = 0.4;
      this.bounces++;
      this.host.particles.sparks(m.pos, 12);
      this.host.sfx.tripleJump();
      break;
    }
  }

  frame(dt: number): void {
    for (const piece of this.pieces.values()) {
      if (piece.pop >= 1) continue;
      piece.pop = Math.min(1, piece.pop + dt * 5);
      const s = 1 + Math.sin(piece.pop * Math.PI) * 0.18;
      piece.mesh.scale.set(s, piece.kind === 'spring' ? 2 - s : s, s);
      if (piece.pop >= 1) piece.mesh.scale.set(1, 1, 1);
    }
    if (!this.drag && !this.stroke && this.pointerPos && this.tool === 'grab') this.hovered = this.pickTarget({ clientX: this.pointerPos.x, clientY: this.pointerPos.y });
    else if (!this.drag) this.hovered = null;
    const target = this.drag?.target ?? this.hovered;
    this.highlight.visible = this.ghost.visible = this.guide.visible = Boolean(target);
    if (!target) return;
    const box = new THREE.Box3().setFromObject(target.object);
    const size = box.getSize(new THREE.Vector3());
    const center = box.getCenter(new THREE.Vector3());
    this.highlight.position.copy(center);
    this.highlight.scale.copy(size.addScalar(12));
    (this.highlight.material as THREE.LineBasicMaterial).color.setHex(this.drag ? 0xffffff : 0xffd24a);
    // A drop line and landing shadow make depth readable while dragging in 3D.
    const floor = this.host.playground.collision.findFloor(center.x, box.min.y - 2, center.z);
    const groundY = floor.surface ? floor.height : box.min.y - 600;
    this.ghost.position.set(center.x, groundY + 3, center.z);
    this.ghost.scale.setScalar(Math.max(60, Math.max(size.x, size.z) * 0.55));
    const positions = this.guide.geometry.getAttribute('position') as THREE.BufferAttribute;
    positions.setXYZ(0, center.x, box.min.y, center.z);
    positions.setXYZ(1, center.x, groundY, center.z);
    positions.needsUpdate = true;
    this.guide.computeLineDistances();
  }

  debug(): Record<string, unknown> {
    return {
      tool: this.tool,
      pieces: [...this.pieces.values()].map(piece => ({ id: piece.id, kind: piece.kind, center: piece.center.toArray(), size: piece.size.toArray(), yaw: piece.yaw, surfaces: piece.surfaces.size })),
      gravity: this.host.mario.gravityScale,
      slowMo: this.slowMo,
      undo: this.undoStack.length,
      bounces: this.bounces,
      portals: this.portals.map(p => p && { center: p.center.toArray(), normal: p.normal.toArray() }),
      warps: this.warps,
      hovered: this.hovered?.id ?? null,
      dragging: this.drag?.target.id ?? null,
    };
  }
}

