/**
 * Third-person orbit camera.  The camera's yaw is the direction from Mario to
 * the camera, which is exactly the convention SM64 uses for
 * `m->area->camera->yaw` in update_mario_joystick_inputs.
 */

import * as THREE from 'three';
import type { CollisionWorld } from '../sim/collision';
import type { SimObject } from '../sim/object';
import { ACT_FLAG_METAL_WATER, ACT_FLAG_ON_POLE, ACT_LONG_JUMP, ACT_SLEEPING, ACT_START_SLEEPING, ACT_TOP_OF_POLE } from '../sim/state';
import { angleToRadians, atan2s, clamp, coss, sins, toS16 } from '../sim/math';

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
  private manualTimer = 0;
  private battlefieldFloor: number | null = null;
  private radialOffset = 0;
  private panDistance = 0;
  private radialTarget = 0;
  private bossFocus: THREE.Vector3 | null = null;
  private waterMode = false;
  private slideMode = false;
  private interiorMode = false;
  private flightMode = false;
  private initialized = false;

  constructor(camera: THREE.PerspectiveCamera, meshes: THREE.Mesh[], includeLargeMeshes = false) {
    this.camera = camera;
    this.setBlockers(meshes, includeLargeMeshes);
  }

  setBlockers(meshes: THREE.Mesh[], includeLargeMeshes = false): void {
    this.blockers.length = 0;
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

  setBattlefieldFloor(surface: number | null): void {
    if (surface === this.battlefieldFloor) return;
    if (surface === null) this.radialOffset = this.radialTarget = 0;
    // The gate's rotate-left triangles set the radial camera offset on entry.
    if (surface === 0x70) this.radialTarget = -Math.PI / 3;
    this.battlefieldFloor = surface;
  }

  private get battlefieldCamera(): boolean {
    return this.battlefieldFloor !== null && this.mode === 'lakitu' && !this.flightMode;
  }

  setBossFocus(position: THREE.Vector3 | null): void {
    if (!position) {
      this.bossFocus = null;
      return;
    }
    this.bossFocus = position;
  }

  setWaterMode(active: boolean): void {
    this.waterMode = active;
  }

  setSlideMode(active: boolean): void {
    this.slideMode = active;
  }

  setInteriorMode(active: boolean): void {
    this.interiorMode = active;
  }

  setFlightMode(active: boolean): void {
    this.flightMode = active;
  }

  get context(): 'radial' | 'normal' | 'boss' | 'water' | 'slide' | 'interior' | 'flight' {
    if (this.bossFocus) return 'boss';
    if (this.waterMode) return 'water';
    if (this.flightMode) return 'flight';
    if (this.slideMode) return 'slide';
    if (this.battlefieldCamera && this.battlefieldFloor !== 0x0b) return 'radial';
    return this.interiorMode ? 'interior' : 'normal';
  }

  get yawS16(): number {
    return toS16(Math.round((this.yaw / (Math.PI * 2)) * 65536));
  }

  displace(dx: number, dy: number, dz: number): void {
    this.focus.set(this.focus.x + dx, this.focus.y + dy, this.focus.z + dz);
    this.camera.position.set(this.camera.position.x + dx, this.camera.position.y + dy, this.camera.position.z + dz);
  }

  snapBehind(faceYaw: number): void {
    this.yaw = ((toS16(faceYaw + 0x8000) * Math.PI * 2) / 65536 + Math.PI * 2) % (Math.PI * 2);
    this.initialized = false;
    this.panDistance = 0;
  }

  rotate(dx: number, dy: number): void {
    if (this.battlefieldCamera) this.radialTarget = clamp(this.radialTarget - dx*0.0042, -Math.PI*2/3, Math.PI*2/3);
    this.yaw -= dx * 0.0042;
    this.pitch = clamp(this.pitch + dy * 0.0034, 0.03, 1.25);
    this.manualTimer = 2;
  }

  orbit(dYaw: number, dPitch: number): void {
    if (this.battlefieldCamera) this.radialTarget = clamp(this.radialTarget + dYaw, -Math.PI*2/3, Math.PI*2/3);
    this.yaw += dYaw;
    this.pitch = clamp(this.pitch + dPitch, 0.03, 1.25);
    this.manualTimer = 2;
  }

  zoom(delta: number): void {
    this.distance = clamp(this.distance + delta * 0.45, 230, 1600);
    this.manualTimer = 2;
  }

  /** Lakitu-style follow: after the player stops orbiting, ease behind Mario. */
  private followYaw(dt: number, marioYaw: number, moving: boolean): void {
    this.manualTimer = Math.max(0, this.manualTimer - dt);
    if (this.manualTimer > 0 || !moving || this.mode === 'topdown' || this.mode === 'side') return;
    const target = this.yawFor(marioYaw + 0x8000);
    let delta = ((target - this.yaw + Math.PI * 3) % (Math.PI * 2)) - Math.PI;
    const rate = this.mode === 'close' ? 1.4 : 0.85;
    const step = clamp(delta, -rate * dt, rate * dt);
    this.yaw += step;
    this.yaw = ((this.yaw % (Math.PI * 2)) + Math.PI * 2) % (Math.PI * 2);
  }

  private yawFor(faceYaw: number): number {
    return ((toS16(faceYaw) * Math.PI * 2) / 65536 + Math.PI * 2) % (Math.PI * 2);
  }

  update(dt: number, marioPos: THREE.Vector3, marioYaw: number, moving = false, speed = 0, opponent?: THREE.Vector3, action = 0, pole?: SimObject | null): void {
    const battlefield = this.battlefieldCamera && !opponent;
    const radial = battlefield && !this.bossFocus && this.battlefieldFloor !== 0x0b;
    if (opponent) {
      const behind = Math.atan2(marioPos.x - opponent.x, marioPos.z - opponent.z);
      const delta = behind - this.yaw;
      this.yaw += Math.atan2(Math.sin(delta),Math.cos(delta)) * (1 - Math.exp(-8 * dt));
    } else if (radial || (battlefield && this.bossFocus)) {
      const rotateStep = (0x200 * Math.PI / 32768) * 30 * dt;
      this.radialOffset += clamp(this.radialTarget - this.radialOffset, -rotateStep, rotateStep);
      // BOB's GEO_CAMERA uses radial mode about (3072, 0, -4608).
      const bearing = this.bossFocus ? Math.PI/4 : Math.atan2(marioPos.x-3072,marioPos.z+4608);
      const delta = bearing + this.radialOffset - this.yaw;
      this.yaw += Math.atan2(Math.sin(delta),Math.cos(delta)) * (this.initialized ? 1-Math.exp(-6*dt) : 1);
    } else this.followYaw(dt, marioYaw, moving);
    if (!this.initialized) {
      this.focus.set(marioPos.x, marioPos.y + 92, marioPos.z);
      this.snapDistances();
      this.initialized = true;
    }

    const target = new THREE.Vector3(marioPos.x, marioPos.y + 92, marioPos.z);
    if (battlefield) target.y = marioPos.y+125;
    let eyeHeightCorrection = 0;
    if (radial && this.world) {
      // calc_y_to_curr_floor uses separate offsets for the camera and its focus.
      const floor = this.world.findFloor(marioPos.x,marioPos.y,marioPos.z).height;
      const surface = action & ACT_FLAG_METAL_WATER ? floor : Math.max(floor,this.world.waterLevel(marioPos.x,marioPos.z));
      const height = surface - marioPos.y;
      const onLowPole = (action & ACT_FLAG_ON_POLE) && pole && floor >= pole.pos[1]
        && marioPos.y < pole.pos[1] + 0.7*pole.hitbox.height;
      const eyeBound = onLowPole ? 1200 : 200;
      const focusOffset = clamp(0.9*height,-200,200);
      target.y += focusOffset;
      eyeHeightCorrection = clamp(height,-eyeBound,eyeBound) - focusOffset;
    }
    const lookAhead = battlefield ? 0 : Math.min(90, Math.max(0, speed) * 2.2);
    target.x += Math.sin((toS16(marioYaw) * Math.PI * 2) / 65536) * lookAhead;
    target.z += Math.cos((toS16(marioYaw) * Math.PI * 2) / 65536) * lookAhead;
    if (opponent) {
      target.set(marioPos.x, marioPos.y + 95, marioPos.z).lerp(opponent, 0.35);
    } else if (this.bossFocus && battlefield) {
      target.copy(marioPos).lerp(this.bossFocus,0.5);
      target.y += 125;
    } else if (this.bossFocus) {
      target.x += (this.bossFocus.x - target.x) * 0.35;
      target.y += (this.bossFocus.y + 80 - target.y) * 0.25;
      target.z += (this.bossFocus.z - target.z) * 0.35;
    }
    this.focus.lerp(target, 1 - Math.exp(-14 * dt));

    let distance = this.distance;
    let pitch = this.pitch;
    if (opponent) {
      distance = clamp(450 + marioPos.distanceTo(opponent) * 0.65, 520, 1600);
      pitch = 0.29;
    } else if (battlefield && this.bossFocus) {
      distance = clamp(marioPos.distanceTo(this.bossFocus)*1.6,800,5000);
      const floor = this.floorAt(3072,20000,-4608) ?? marioPos.y;
      pitch = Math.atan2(floor+550-this.focus.y,distance*Math.cos(Math.PI/8));
      distance = distance*Math.cos(Math.PI/8)/Math.cos(pitch);
    } else if (radial) {
      distance += 100;
      const yaw = this.yawS16;
      const probe = this.world?.findFloor(marioPos.x+40*sins(yaw),marioPos.y,marioPos.z+40*coss(yaw));
      const rise = probe ? probe.height-marioPos.y : 0;
      const slope = probe?.surface && probe.surface.type !== 0x28 && rise > 0 &&
        (probe.surface.normal[2] !== 0 || rise >= 100) ? atan2s(40,rise) : 0;
      pitch = this.pitch-0.31 + angleToRadians(0x5b0+slope);
    } else if (battlefield) {
      distance = Math.max(230,distance-100);
    } else if (this.mode === 'lakitu' && this.bossFocus) {
      distance = Math.max(distance, 1100);
      pitch = Math.max(pitch, 0.36);
    } else if (this.mode === 'lakitu' && this.waterMode) {
      distance = Math.min(distance, 700);
      pitch = Math.min(pitch, 0.18);
    } else if (this.mode === 'lakitu' && this.flightMode) {
      distance = Math.max(distance, 980);
      pitch = Math.max(pitch, 0.28);
    } else if (this.mode === 'lakitu' && this.slideMode) {
      distance = Math.min(distance, 620);
      pitch = Math.min(pitch, 0.14);
    } else if (this.mode === 'lakitu' && this.interiorMode) {
      distance = Math.min(distance, 620);
      pitch = Math.min(pitch, 0.2);
    }
    const cp = Math.cos(pitch);
    const sp = Math.sin(pitch);
    this.desired.set(
      this.focus.x + Math.sin(this.yaw) * cp * distance,
      this.focus.y + eyeHeightCorrection + sp * distance,
      this.focus.z + Math.cos(this.yaw) * cp * distance,
    );

    // Pull the camera in if a wall is between it and Mario.
    this.direction.copy(this.desired).sub(this.focus);
    const length = this.direction.length();
    let allowed = length;
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

    const scale = length ? this.smoothingDistance / length : 0;
    this.desired.set(
      this.focus.x + Math.sin(this.yaw) * cp * distance * scale,
      this.focus.y + (eyeHeightCorrection + sp * distance) * scale,
      this.focus.z + Math.cos(this.yaw) * cp * distance * scale,
    );

    // The orbit focus and unobstructed extension are already smoothed. A
    // second world-position lerp can put the final eye behind the hit surface.
    if (battlefield) {
      this.desired.x = clamp(this.desired.x,-8000,8000);
      this.desired.z = clamp(this.desired.z,-8000,7050);
      this.yaw = Math.atan2(this.desired.x-this.focus.x,this.desired.z-this.focus.z);
    }
    this.camera.position.copy(this.desired);

    // Keep the camera above the terrain.
    const floor = this.floorAt(this.camera.position.x, this.camera.position.y, this.camera.position.z);
    if (floor !== null && this.camera.position.y < floor + 30) {
      this.camera.position.y = floor + 30;
    }
    if (radial) {
      const cameraYaw = atan2s(marioPos.z-this.camera.position.z, marioPos.x-this.camera.position.x);
      const side = sins(marioYaw)*coss(cameraYaw)-coss(marioYaw)*sins(cameraYaw);
      const reverse = action === ACT_LONG_JUMP || ((action & ACT_FLAG_ON_POLE) && action !== ACT_TOP_OF_POLE);
      const sleeping = action === ACT_SLEEPING || action === ACT_START_SLEEPING;
      const pan = sleeping ? 0 : side*sins(0xc00)*this.camera.position.distanceTo(marioPos)*(reverse ? -1 : 1);
      this.panDistance += (pan-this.panDistance)*(1-Math.pow(0.975,30*dt));
      target.copy(this.focus);
      target.x += this.panDistance*coss(cameraYaw);
      target.z -= this.panDistance*sins(cameraYaw);
      this.camera.lookAt(target);
    } else {
      this.panDistance = 0;
      this.camera.lookAt(this.focus);
    }
  }

  private world?: CollisionWorld;

  setCollisionWorld(world: CollisionWorld): void {
    this.world = world;
  }

  private floorAt(x: number, y: number, z: number): number | null {
    const hit = this.world?.findFloor(x,y,z);
    return hit?.surface ? hit.height : null;
  }

  private snapDistances(): void {
    this.smoothingDistance = this.distance;
  }
}
