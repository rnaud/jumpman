import * as THREE from 'three';
import { KOOPA_RACE_ROUTE } from '../../sim/behaviors/koopaTheQuick';
import { angleToRadians, atan2s, coss, sins, toS16 } from '../../sim/math';
import type { CourseId } from '../courses';
import { marioRoute } from './crowdRoute';
import { RailPath, type RailHit } from './railPath';
import type { Experiment, ExperimentHost, StepOverride } from './types';
import { dock, segmented, titleCard, type Segmented } from './ui';

export type DimensionMode = '3d' | 'side' | 'top' | 'path';
const LABEL: Record<DimensionMode, string> = { '3d': '3D', side: '2D side', top: 'Top-down', path: '2.5D path' };
/** 2.5D: a side-on perspective camera that swings with the path (Klonoa, Pandemonium). */
const PATH_FOV = 40;
const PATH_DISTANCE = 1250;
const PATH_HEIGHT = 260;

/** Near-orthographic lenses: a narrow FOV from far away flattens perspective. */
const FLAT_FOV = 7;
const SIDE_VIEW_HEIGHT = 1500;
const TOP_VIEW_HEIGHT = 2600;
/** Geometry this close in front of the lane stays visible (Mario's collision radius plus margin). */
const LANE_MARGIN = 70;
const TRANSITION = 0.75;

interface Pose {
  position: THREE.Vector3;
  up: THREE.Vector3;
  fov: number;
}

const smooth = (t: number) => t * t * (3 - 2 * t);

function makeGuide(count = 28): THREE.InstancedMesh {
  const mesh = new THREE.InstancedMesh(
    new THREE.SphereGeometry(13, 10, 6),
    new THREE.MeshBasicMaterial({ color: 0xffd24a }),
    count,
  );
  mesh.frustumCulled = false;
  mesh.visible = false;
  mesh.count = 0;
  mesh.name = 'dimension-path-guide';
  return mesh;
}

/** A ribbon of columns (grass edge, earth, deep earth) for the side-view cross-section. */
function makeProfile(columns = 160): THREE.Mesh {
  const positions = new Float32Array(columns * 3 * 3);
  const colors = new Float32Array(columns * 3 * 3);
  const grass = new THREE.Color(0x3f9a32), earth = new THREE.Color(0x7a4a26), deep = new THREE.Color(0x2e1a0e);
  for (let i = 0; i < columns; i++) {
    grass.toArray(colors, i * 9);
    earth.toArray(colors, i * 9 + 3);
    deep.toArray(colors, i * 9 + 6);
  }
  const index: number[] = [];
  for (let i = 0; i < columns - 1; i++) {
    const a = i * 3, b = (i + 1) * 3;
    index.push(a, a + 1, b, b, a + 1, b + 1, a + 1, a + 2, b + 1, b + 1, a + 2, b + 2);
  }
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
  geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
  geometry.setIndex(index);
  const mesh = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ vertexColors: true, side: THREE.DoubleSide }));
  mesh.frustumCulled = false;
  mesh.visible = false;
  mesh.name = 'dimension-profile';
  return mesh;
}

/** Visible height of a camera at `distance` with vertical FOV in degrees. */
export function viewHeight(distance: number, fov: number): number {
  return 2 * distance * Math.tan(THREE.MathUtils.degToRad(fov) / 2);
}
export function distanceForHeight(height: number, fov: number): number {
  return height / (2 * Math.tan(THREE.MathUtils.degToRad(fov) / 2));
}

/** Lane direction for a side view: the camera's screen-right, snapped to 45°. */
export function laneYawFromCamera(cameraForwardYaw: number): number {
  return toS16(Math.round(toS16(cameraForwardYaw - 0x4000) / 0x2000) * 0x2000);
}

/**
 * Mario Across Dimensions: the same course and the same physics, seen and
 * played as 3D, a side-scrolling cross-section, or a top-down map.
 */
export interface DimensionsOptions {
  course?: CourseId | null;
  initial?: DimensionMode;
  /** Force the side-scrolling axis (authored 2D levels). */
  laneYaw?: number;
}

export class DimensionsExperiment implements Experiment {
  readonly id = 'dimensions';
  readonly title = 'Across Dimensions';
  readonly course: CourseId | null;
  private readonly initial: DimensionMode;
  private readonly forcedLane: number | undefined;
  private host!: ExperimentHost;
  mode: DimensionMode = '3d';
  private laneYaw = 0x4000;
  private readonly laneOrigin = new THREE.Vector3();
  private topYaw = 0;
  private from: Pose | null = null;
  private elapsed = TRANSITION;
  private baseFov = 55;
  private baseNear = 1;
  private baseFar = 40000;
  private baseBackground: THREE.Scene['background'] = null;
  private readonly earth = new THREE.Color(0x4b2f1c);
  private focusY = 0;
  private clipY = 0;
  private readonly plane = new THREE.Plane();
  private seg!: Segmented<DimensionMode>;
  private switches = 0;
  /** The cut face of the level along the clip plane, rebuilt every frame from collision floors. */
  private readonly profile = makeProfile();
  private modes: DimensionMode[] = ['3d', 'side', 'top'];
  /** The 2.5D rail (the course's race route, re-planned so Mario can walk it). */
  private rail: RailPath | null = null;
  private railHit: RailHit | null = null;
  /** +1: camera on the rail's right so progress runs screen-right; -1: the other side. */
  private railSide = 1;
  private railYaw = 0;
  private readonly guide = makeGuide();
  private summitAnnounced = false;

  constructor(options: DimensionsOptions = {}) {
    this.course = options.course === undefined ? 'bob' : options.course;
    this.initial = options.initial ?? '3d';
    this.forcedLane = options.laneYaw;
  }

  enter(host: ExperimentHost): void {
    this.host = host;
    host.scene.add(this.profile, this.guide);
    if (host.worldName === 'bob') {
      this.rail = new RailPath(marioRoute(host.playground.collision, KOOPA_RACE_ROUTE));
      this.railSide = this.chooseRailSide();
      this.modes = ['3d', 'side', 'top', 'path'];
    }
    this.baseFov = host.camera.fov;
    this.baseNear = host.camera.near;
    this.baseFar = host.camera.far;
    this.baseBackground = host.scene.background;
    titleCard(host.ui, '01', 'Across Dimensions', 'One level, one Mario, many ways to play it. Switch any time — even mid-jump.', [
      ['Tab', 'next dimension'], ['1–4', '3D · 2D · top-down · 2.5D path'], ['A / D', 'run (2D · path)'], ['WASD', 'run (3D · top-down)'],
    ]);
    const bar = dock(host.ui);
    this.seg = segmented(this.modes.map((mode, i) => ({ value: mode, label: LABEL[mode], key: String(i + 1) })), this.mode, mode => this.setMode(mode));
    bar.append(this.seg.element);
    if (this.initial !== '3d') {
      this.setMode(this.initial);
      // Start already flat: no transition from the 3D camera.
      this.from = null;
    } else host.toast('PRESS TAB TO FLATTEN THE WORLD', 3200);
  }

  exit(): void {
    const { host } = this;
    host.renderer.clippingPlanes = [];
    host.mario.movementMode = 'default';
    this.profile.removeFromParent();
    this.guide.removeFromParent();
    this.guide.geometry.dispose();
    (this.guide.material as THREE.Material).dispose();
    this.profile.geometry.dispose();
    (this.profile.material as THREE.Material).dispose();
    host.scene.background = this.baseBackground;
    host.camera.fov = this.baseFov;
    host.camera.near = this.baseNear;
    host.camera.far = this.baseFar;
    host.camera.up.set(0, 1, 0);
    host.camera.updateProjectionMatrix();
  }

  keydown(event: KeyboardEvent): boolean {
    if (event.code === 'Tab' || event.code === 'KeyM') {
      event.preventDefault();
      this.setMode(this.modes[(this.modes.indexOf(this.mode) + 1) % this.modes.length]);
      return true;
    }
    const index = ['Digit1', 'Digit2', 'Digit3', 'Digit4'].indexOf(event.code);
    if (index >= 0 && index < this.modes.length) {
      this.setMode(this.modes[index]);
      return true;
    }
    return false;
  }

  setMode(mode: DimensionMode): void {
    if (mode === this.mode) return;
    const { camera, mario, rig } = this.host;
    // Transitions start from wherever the camera is right now, so a switch mid-transition stays smooth.
    this.from = { position: camera.position.clone(), up: camera.up.clone(), fov: camera.fov };
    this.elapsed = 0;
    const forwardYaw = toS16(rig.yawS16 + 0x8000);
    if (mode === 'side') {
      this.laneYaw = this.forcedLane ?? (this.mode === 'top' ? laneYawFromCamera(this.topYaw) : laneYawFromCamera(forwardYaw));
      this.laneOrigin.fromArray(mario.pos);
      mario.sideYaw = this.laneYaw;
      mario.movementMode = 'side';
    } else if (mode === 'path' && this.rail) {
      this.joinRail();
      mario.movementMode = 'side';
    } else {
      mario.movementMode = 'default';
    }
    if (mode === 'top') {
      this.topYaw = this.mode === 'side' ? toS16(this.laneYaw + 0x4000) : forwardYaw;
      this.clipY = mario.pos[1] + 420;
    }
    this.focusY = mario.pos[1] + 120;
    this.mode = mode;
    // Looking down into sliced hills shows their hollow inside: make that read as earth.
    this.host.scene.background = mode === 'top' ? this.earth : this.baseBackground;
    this.switches++;
    this.seg.set(mode);
    this.host.toast(mode === '3d' ? 'BACK TO 3D' : mode === 'side' ? '2D · SIDE-SCROLLING CROSS-SECTION'
      : mode === 'path' ? '2.5D · FOLLOW THE PATH TO THE SUMMIT' : 'TOP-DOWN', 1600);
  }

  /** Step onto the rail where it is closest (or at its start when Mario is far away). */
  private joinRail(): void {
    const rail = this.rail!;
    const m = this.host.mario;
    let hit = rail.nearest(m.pos);
    const away = Math.hypot(hit.point[0] - m.pos[0], hit.point[2] - m.pos[2]);
    if (away > 500) {
      hit = rail.at(0);
      this.host.placeMario([hit.point[0], hit.point[1] + 200, hit.point[2]], atan2s(hit.tangent[1], hit.tangent[0]));
    }
    this.railHit = hit;
    this.railYaw = atan2s(hit.tangent[1], hit.tangent[0]);
    this.summitAnnounced = false;
  }

  /**
   * Put the camera on whichever side of the route has lower ground more often,
   * so it looks across the mountain path instead of into the hillside.
   */
  private chooseRailSide(): number {
    const rail = this.rail!;
    const world = this.host.playground.collision;
    let vote = 0;
    for (let s = 0; s < rail.total; s += 300) {
      const hit = rail.at(s);
      const [tx, tz] = hit.tangent;
      // Camera at +1 sits on the tangent's left-hand side (screen-right = tangent).
      const left = world.findFloor(hit.point[0] - tz * 700, hit.point[1] + 3000, hit.point[2] + tx * 700).height;
      const right = world.findFloor(hit.point[0] + tz * 700, hit.point[1] + 3000, hit.point[2] - tx * 700).height;
      vote += Math.sign(right - left);
    }
    return vote >= 0 ? 1 : -1;
  }

  beforeStep(): StepOverride | void {
    // Top-down: the stick is screen-relative to the fixed map orientation.
    if (this.mode === 'top') return { cameraYaw: toS16(this.topYaw + 0x8000) };
  }

  afterStep(): void {
    if (this.mode === 'path') { this.followRail(); return; }
    if (this.mode !== 'side') return;
    // Keep Mario on the lane: remove any drift across it (wall pushes, knockbacks).
    const m = this.host.mario;
    const lx = sins(this.laneYaw), lz = coss(this.laneYaw);
    const ox = m.pos[0] - this.laneOrigin.x, oz = m.pos[2] - this.laneOrigin.z;
    const along = ox * lx + oz * lz;
    m.pos[0] = this.laneOrigin.x + lx * along;
    m.pos[2] = this.laneOrigin.z + lz * along;
    const velocityAlong = m.vel[0] * lx + m.vel[2] * lz;
    m.vel[0] = lx * velocityAlong;
    m.vel[2] = lz * velocityAlong;
  }

  /** Project Mario onto the rail and aim his side-scrolling stick along it. */
  private followRail(): void {
    const rail = this.rail;
    if (!rail) return;
    const m = this.host.mario;
    const hit = rail.nearest(m.pos, this.railHit?.index ?? -1);
    this.railHit = hit;
    const [tx, tz] = hit.tangent;
    m.pos[0] = hit.point[0];
    m.pos[2] = hit.point[2];
    const along = m.vel[0] * tx + m.vel[2] * tz;
    m.vel[0] = tx * along;
    m.vel[2] = tz * along;
    const tangentYaw = atan2s(tz, tx);
    // Stick right always moves screen-right.
    m.sideYaw = this.railSide > 0 ? tangentYaw : toS16(tangentYaw + 0x8000);
    if (!this.summitAnnounced && hit.s > rail.total - 250) {
      this.summitAnnounced = true;
      this.host.toast('★ SUMMIT ★', 2000);
      this.host.sfx.starSound();
    }
  }

  frame(dt: number): void {
    const { camera, renderPos } = this.host;
    const t = this.from ? smooth(Math.min(1, (this.elapsed += dt) / TRANSITION)) : 1;
    const focus = new THREE.Vector3(renderPos.x, renderPos.y + 110, renderPos.z);
    let target: Pose;
    if (this.mode === '3d') {
      // The follow camera already placed itself this frame; blend toward it.
      target = { position: camera.position.clone(), up: new THREE.Vector3(0, 1, 0), fov: this.baseFov };
      if (t >= 1) {
        this.from = null;
        this.host.renderer.clippingPlanes = [];
        if (camera.near !== this.baseNear || camera.far !== this.baseFar) {
          camera.near = this.baseNear;
          camera.far = this.baseFar;
          camera.updateProjectionMatrix();
        }
        return;
      }
    } else if (this.mode === 'side') {
      // Platformer camera: exact along the lane, lazy vertically.
      const desiredY = renderPos.y + 140;
      if (Math.abs(desiredY - this.focusY) > 160) this.focusY += (desiredY - this.focusY - Math.sign(desiredY - this.focusY) * 160) * Math.min(1, dt * 6);
      this.focusY += (desiredY - this.focusY) * Math.min(1, dt * 1.6);
      focus.y = this.focusY;
      const view = angleToRadians(this.laneYaw + 0x4000);
      const distance = distanceForHeight(SIDE_VIEW_HEIGHT, FLAT_FOV);
      target = {
        position: focus.clone().add(new THREE.Vector3(-Math.sin(view) * distance, 0, -Math.cos(view) * distance)),
        up: new THREE.Vector3(0, 1, 0),
        fov: FLAT_FOV,
      };
      // Clip everything between the camera and the lane.
      const normal = new THREE.Vector3(Math.sin(view), 0, Math.cos(view));
      const lanePoint = new THREE.Vector3(renderPos.x, 0, renderPos.z);
      this.plane.set(normal, -normal.dot(lanePoint) + LANE_MARGIN);
      // The cut appears once the lens is mostly flat, so it never pops in mid-perspective.
      this.host.renderer.clippingPlanes = t > 0.45 ? [this.plane] : [];
      this.updateProfile(lanePoint, normal, renderPos.y);
    } else if (this.mode === 'path' && this.railHit) {
      const [tx, tz] = this.railHit.tangent;
      // Swing smoothly around bends rather than snapping with every rail sample.
      const tangentYaw = atan2s(tz, tx);
      this.railYaw = toS16(this.railYaw + toS16(tangentYaw - this.railYaw) * Math.min(1, dt * 3));
      const view = angleToRadians(this.railYaw + this.railSide * 0x4000);
      const lead = angleToRadians(this.railYaw);
      const desiredY = renderPos.y + 120;
      this.focusY += (desiredY - this.focusY) * Math.min(1, dt * 3);
      focus.set(renderPos.x + Math.sin(lead) * 120 * this.railSide, this.focusY, renderPos.z + Math.cos(lead) * 120 * this.railSide);
      target = {
        position: focus.clone().add(new THREE.Vector3(-Math.sin(view) * PATH_DISTANCE, PATH_HEIGHT, -Math.cos(view) * PATH_DISTANCE)),
        up: new THREE.Vector3(0, 1, 0),
        fov: PATH_FOV,
      };
      // Hillsides between the camera and the path are cut away so Mario is always in view.
      const normal = new THREE.Vector3(Math.sin(view), 0, Math.cos(view));
      this.plane.set(normal, -normal.dot(new THREE.Vector3(renderPos.x, 0, renderPos.z)) + 260);
      this.host.renderer.clippingPlanes = t > 0.4 ? [this.plane] : [];
      this.updateGuide();
    } else {
      focus.y = renderPos.y;
      const up = angleToRadians(this.topYaw);
      const distance = distanceForHeight(TOP_VIEW_HEIGHT, FLAT_FOV);
      target = {
        position: focus.clone().add(new THREE.Vector3(0, distance, 0)),
        up: new THREE.Vector3(Math.sin(up), 0, Math.cos(up)),
        fov: FLAT_FOV,
      };
      // Slice off ceilings, treetops and overhangs above Mario's head.
      const ceiling = renderPos.y + 420;
      this.clipY += (ceiling - this.clipY) * Math.min(1, dt * 4);
      this.plane.set(new THREE.Vector3(0, -1, 0), this.clipY);
      this.host.renderer.clippingPlanes = [this.plane];
    }

    this.profile.visible = this.mode === 'side' && t > 0.45;
    this.guide.visible = this.mode === 'path';
    const pose = this.from && t < 1 ? this.blend(this.from, target, focus, t) : target;
    if (t >= 1) this.from = null;
    camera.position.copy(pose.position);
    camera.up.copy(pose.up);
    camera.fov = pose.fov;
    const distance = pose.position.distanceTo(focus);
    // A far-away narrow lens needs its depth range pulled in to avoid z-fighting.
    camera.near = Math.max(this.baseNear, distance - 3000);
    camera.far = Math.max(this.baseFar, distance + 26000);
    camera.updateProjectionMatrix();
    camera.lookAt(focus);
  }

  /** Glowing dots along the path ahead show where the route goes next. */
  private updateGuide(): void {
    const rail = this.rail, hit = this.railHit;
    if (!rail || !hit) return;
    const world = this.host.playground.collision;
    const matrix = new THREE.Matrix4();
    const count = this.guide.instanceMatrix.count;
    let n = 0;
    const start = Math.floor(hit.s / 140) * 140;
    for (let i = -3; i < count - 3 && n < count; i++) {
      const s = start + i * 140 * this.railSide;
      if (s < 0 || s > rail.total) continue;
      const p = rail.at(s).point;
      const floor = world.findFloor(p[0], Math.max(p[1], this.host.renderPos.y) + 600, p[2]);
      if (!floor.surface) continue;
      const pulse = 1 + 0.25 * Math.sin(this.host.time * 5 - i * 0.6);
      matrix.makeScale(pulse, pulse, pulse).setPosition(p[0], floor.height + 14, p[2]);
      this.guide.setMatrixAt(n++, matrix);
    }
    this.guide.count = n;
    this.guide.instanceMatrix.needsUpdate = true;
  }

  private updateProfile(lanePoint: THREE.Vector3, normal: THREE.Vector3, marioY: number): void {
    const world = this.host.playground.collision;
    const along = new THREE.Vector3(sins(this.laneYaw), 0, coss(this.laneYaw));
    // Sit just inside the kept half-space so the clip plane does not eat it.
    const cut = lanePoint.clone().addScaledVector(normal, -LANE_MARGIN + 2);
    const position = this.profile.geometry.getAttribute('position') as THREE.BufferAttribute;
    const columns = position.count / 3;
    const half = 3200;
    let last = marioY - 400;
    for (let i = 0; i < columns; i++) {
      const s = -half + (i / (columns - 1)) * half * 2;
      const x = cut.x + along.x * s, z = cut.z + along.z * s;
      // Heights come from Mario's own lane, so the slice's top edge is the ground he runs on.
      const lx = lanePoint.x + along.x * s, lz = lanePoint.z + along.z * s;
      const floor = world.findFloor(lx, Math.max(marioY, last) + 700, lz);
      const top = floor.surface ? floor.height : last - 2000;
      if (floor.surface) last = top;
      position.setXYZ(i * 3, x, top, z);
      position.setXYZ(i * 3 + 1, x, top - 40, z);
      position.setXYZ(i * 3 + 2, x, Math.min(top, marioY) - 6000, z);
    }
    position.needsUpdate = true;
    this.profile.geometry.computeBoundingSphere();
  }

  /** Dolly-zoom between poses: the visible height around Mario is preserved while the lens narrows. */
  private blend(from: Pose, to: Pose, focus: THREE.Vector3, t: number): Pose {
    const fromOffset = from.position.clone().sub(focus);
    const toOffset = to.position.clone().sub(focus);
    const fromHeight = viewHeight(fromOffset.length(), from.fov);
    const toHeight = viewHeight(toOffset.length(), to.fov);
    const fov = Math.exp(THREE.MathUtils.lerp(Math.log(from.fov), Math.log(to.fov), t));
    const height = THREE.MathUtils.lerp(fromHeight, toHeight, t);
    const a = fromOffset.normalize();
    const b = toOffset.normalize();
    const rotation = new THREE.Quaternion().setFromUnitVectors(a, b);
    const partial = new THREE.Quaternion().slerp(rotation, t);
    const direction = a.clone().applyQuaternion(partial).normalize();
    const up = from.up.clone().lerp(to.up, t);
    if (up.lengthSq() < 1e-4) up.copy(to.up);
    return { position: focus.clone().addScaledVector(direction, distanceForHeight(height, fov)), up: up.normalize(), fov };
  }

  debug(): Record<string, unknown> {
    return {
      mode: this.mode,
      laneYaw: this.laneYaw,
      topYaw: this.topYaw,
      switches: this.switches,
      transitioning: this.from !== null,
      clipping: this.host.renderer.clippingPlanes.length,
      modes: this.modes,
      rail: this.rail ? { length: Math.round(this.rail.total), s: Math.round(this.railHit?.s ?? -1), side: this.railSide } : null,
      fov: this.host.camera.fov,
    };
  }
}

