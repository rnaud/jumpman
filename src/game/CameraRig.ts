/**
 * Third-person orbit camera.  The camera's yaw is the direction from Mario to
 * the camera, which is exactly the convention SM64 uses for
 * `m->area->camera->yaw` in update_mario_joystick_inputs.
 */

import * as THREE from 'three';
import { clamp, toS16 } from '../sim/math';

export type CameraMode = 'lakitu' | 'close' | 'topdown' | 'side';

export class CameraRig {
  readonly camera: THREE.PerspectiveCamera;

  yaw = 0;
  pitch = 0.31;
  distance = 900;
  mode: CameraMode = 'lakitu';

  private readonly focus = new THREE.Vector3();
  private readonly desired = new THREE.Vector3();
  private readonly direction = new THREE.Vector3();
  private readonly raycaster = new THREE.Raycaster();
  private readonly blockers: THREE.Mesh[] = [];
  private smoothingDistance = this.distance;
  private initialized = false;

  constructor(camera: THREE.PerspectiveCamera, meshes: THREE.Mesh[], includeLargeMeshes = false) {
    this.camera = camera;

    for (const mesh of meshes) {
      mesh.geometry.computeBoundingSphere();
      const radius = mesh.geometry.boundingSphere?.radius ?? 0;
      if (includeLargeMeshes || radius < 2200) this.blockers.push(mesh);
    }
  }

  setMode(mode: CameraMode): void {
    this.mode = mode;
    const presets: Record<CameraMode, { distance: number; pitch: number }> = {
      lakitu: { distance: 900, pitch: 0.31 },
      close: { distance: 480, pitch: 0.22 },
      topdown: { distance: 1350, pitch: 1.18 },
      side: { distance: 900, pitch: 0.13 },
    };
    this.distance = presets[mode].distance;
    this.pitch = presets[mode].pitch;
    this.smoothingDistance = this.distance;
  }

  cycleMode(): CameraMode {
    const order: CameraMode[] = ['lakitu', 'close', 'topdown', 'side'];
    this.setMode(order[(order.indexOf(this.mode) + 1) % order.length]);
    return this.mode;
  }

  get yawS16(): number {
    return toS16(Math.round((this.yaw / (Math.PI * 2)) * 65536));
  }

  snapBehind(faceYaw: number): void {
    this.yaw = ((toS16(faceYaw + 0x8000) * Math.PI * 2) / 65536 + Math.PI * 2) % (Math.PI * 2);
    this.initialized = false;
  }

  rotate(dx: number, dy: number): void {
    this.yaw -= dx * 0.0042;
    this.pitch = clamp(this.pitch + dy * 0.0034, 0.03, 1.25);
  }

  orbit(dYaw: number, dPitch: number): void {
    this.yaw += dYaw;
    this.pitch = clamp(this.pitch + dPitch, 0.03, 1.25);
  }

  zoom(delta: number): void {
    this.distance = clamp(this.distance + delta * 0.45, 230, 1600);
  }

  update(dt: number, marioPos: THREE.Vector3, marioYaw: number): void {
    if (!this.initialized) {
      this.focus.set(marioPos.x, marioPos.y + 92, marioPos.z);
      this.snapDistances();
      this.initialized = true;
    }

    const target = new THREE.Vector3(marioPos.x, marioPos.y + 92, marioPos.z);
    this.focus.lerp(target, 1 - Math.exp(-14 * dt));

    const cp = Math.cos(this.pitch);
    const sp = Math.sin(this.pitch);
    this.desired.set(
      this.focus.x + Math.sin(this.yaw) * cp * this.distance,
      this.focus.y + sp * this.distance,
      this.focus.z + Math.cos(this.yaw) * cp * this.distance,
    );

    // Pull the camera in if a wall is between it and Mario.
    let allowed = this.distance;
    this.direction.copy(this.desired).sub(this.focus);
    const length = this.direction.length();
    if (length > 1) {
      this.direction.multiplyScalar(1 / length);
      this.raycaster.set(this.focus, this.direction);
      this.raycaster.near = 0;
      this.raycaster.far = length;
      const hits = this.raycaster.intersectObjects(this.blockers, false);
      if (hits.length > 0) {
        allowed = Math.max(0.01, hits[0].distance - 22);
      }
    }

    if (allowed < this.smoothingDistance) {
      this.smoothingDistance = allowed;
    } else {
      this.smoothingDistance += (allowed - this.smoothingDistance) * (1 - Math.exp(-5 * dt));
    }

    const scale = this.smoothingDistance / this.distance;
    this.desired.set(
      this.focus.x + Math.sin(this.yaw) * cp * this.distance * scale,
      this.focus.y + sp * this.distance * scale,
      this.focus.z + Math.cos(this.yaw) * cp * this.distance * scale,
    );

    // The orbit focus and unobstructed extension are already smoothed. A
    // second world-position lerp can put the final eye behind the hit surface.
    this.camera.position.copy(this.desired);
    this.camera.lookAt(this.focus);

    // Keep the camera above the terrain.
    const floor = this.floorAt(this.camera.position.x, this.camera.position.y, this.camera.position.z);
    if (floor !== null && this.camera.position.y < floor + 30) {
      this.camera.position.y = floor + 30;
      this.camera.lookAt(this.focus);
    }
  }

  /** Cheap floor probe so the camera does not dip below the ground. */
  private floorProbe: ((x: number, y: number, z: number) => number | null) | null = null;

  setFloorProbe(fn: (x: number, y: number, z: number) => number | null): void {
    this.floorProbe = fn;
  }

  private floorAt(x: number, y: number, z: number): number | null {
    return this.floorProbe ? this.floorProbe(x, y, z) : null;
  }

  private snapDistances(): void {
    this.smoothingDistance = this.distance;
  }
}
