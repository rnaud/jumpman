/**
 * Third-person orbit camera.  The camera's yaw is the direction from Mario to
 * the camera, which is exactly the convention SM64 uses for
 * `m->area->camera->yaw` in update_mario_joystick_inputs.
 */

import * as THREE from 'three';
import { CameraTransition } from './CameraTransition';
import { radialWallYaw } from './cameraWall';
import { SummitCamera } from './SummitCamera';
import type { CollisionWorld } from '../sim/collision';
import type { SimObject } from '../sim/object';
import { ACT_FLAG_HANGING, ACT_FLAG_METAL_WATER, ACT_FLAG_ON_POLE, ACT_LONG_JUMP, ACT_SLEEPING, ACT_START_SLEEPING, ACT_TOP_OF_POLE } from '../sim/state';
import { angleToRadians, atan2s, clamp, coss, sins, toS16, type Vec3 } from '../sim/math';

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
  private radialTicks = 0;
  private radialReturn?: CameraTransition;
  private leavingSummit = false;
  private readonly renderedRadialEye = new THREE.Vector3();
  private lastRadialEye?: THREE.Vector3;
  private panDistance = 0;
  private radialTarget = 0;
  private bossFocus: THREE.Vector3 | null = null;
  private summit?: SummitCamera;
  private bossHeld = false;
  private waterMode = false;
  private slideMode = false;
  private interiorMode = false;
  private flightMode = false;
  private initialized = false;

  constructor(camera: THREE.PerspectiveCamera, meshes: THREE.Mesh[], includeLargeMeshes = false) {
    this.camera = camera;
    this.setBlockers(meshes, includeLargeMeshes);
  }

  copyFocus(): THREE.Vector3 { return this.focus.clone(); }

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
    if (surface === 0x6f) this.radialTarget = Math.PI / 3;
    if (surface === 0x6e) this.radialTarget = 0;
    this.battlefieldFloor = surface;
  }

  private get battlefieldCamera(): boolean {
    return this.battlefieldFloor !== null && this.mode === 'lakitu' && !this.flightMode;
  }

  setBossFocus(position: THREE.Vector3 | null, held = false): void {
    this.bossHeld = held;
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
    if (this.lastRadialEye) {
      this.renderedRadialEye.x += dx;
      this.renderedRadialEye.y += dy;
      this.renderedRadialEye.z += dz;
      this.lastRadialEye.x += dx;
      this.lastRadialEye.y += dy;
      this.lastRadialEye.z += dz;
    }
    this.focus.set(this.focus.x + dx, this.focus.y + dy, this.focus.z + dz);
    this.camera.position.set(this.camera.position.x + dx, this.camera.position.y + dy, this.camera.position.z + dz);
  }

  snapBehind(faceYaw: number): void {
    this.yaw = ((toS16(faceYaw + 0x8000) * Math.PI * 2) / 65536 + Math.PI * 2) % (Math.PI * 2);
    this.initialized = false;
    this.summit = undefined;
    this.lastRadialEye = undefined;
    this.radialTicks = 0;
    this.radialReturn = undefined;
    this.leavingSummit = false;
    this.panDistance = 0;
  }

  rotate(dx: number, dy: number): void {
    if (this.summit) { this.summit.drag(dx,dy); return; }
    if (this.battlefieldCamera) this.radialTarget = clamp(this.radialTarget - dx*0.0042, -Math.PI*2/3, Math.PI*2/3);
    this.yaw -= dx * 0.0042;
    this.pitch = clamp(this.pitch + dy * 0.0034, 0.03, 1.25);
    this.manualTimer = 2;
  }

  orbit(dYaw: number, dPitch: number): void {
    if (this.summit) { this.summit.orbit(dYaw,dPitch); return; }
    if (!dYaw && !dPitch) return;
    if (this.battlefieldCamera) this.radialTarget = clamp(this.radialTarget + dYaw, -Math.PI*2/3, Math.PI*2/3);
    this.yaw += dYaw;
    this.pitch = clamp(this.pitch + dPitch, 0.03, 1.25);
    this.manualTimer = 2;
  }

  zoom(delta: number): void {
    if (this.summit) { this.summit.zoomedOut = delta>0; return; }
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

  update(dt: number, marioPos: THREE.Vector3, marioYaw: number, moving = false, speed = 0, opponent?: THREE.Vector3, action = 0, pole?: SimObject | null, marioPitch = 0, spin = 0): void {
    const battlefield = this.battlefieldCamera && !opponent;
    const radial = battlefield && !this.bossFocus && this.battlefieldFloor !== 0x0b;
    if (!radial) {
      this.lastRadialEye = undefined;
      this.radialReturn = undefined;
      this.leavingSummit = false;
    }
    if (battlefield && this.bossFocus) {
      if (!this.initialized) {
        this.focus.copy(marioPos).y += 125;
        this.camera.position.copy(this.focus).z += this.distance;
        this.initialized = true;
      }
      this.summit ??= new SummitCamera(this.camera.position,this.focus,this.yaw);
      this.summit.update(dt,marioPos,this.bossFocus,this.bossHeld,marioYaw,marioPitch,spin,this.camera.position,this.focus,this.world);
      this.camera.lookAt(this.focus);
      this.yaw = this.summit.yaw;
      return;
    }
    if (this.summit) {
      this.leavingSummit = radial;
      this.radialOffset = this.radialTarget = 0;
      this.summit = undefined;
    }
    if (radial) {
      this.radialTicks += dt*30;
      if (!this.lastRadialEye) this.radialTicks = Math.max(1,this.radialTicks);
      while (this.radialTicks >= 1) {
        this.radialTicks--;
        this.updateRadial(marioPos,marioYaw,speed,action,pole);
      }
      this.camera.position.copy(this.renderedRadialEye);
      this.camera.lookAt(this.focus);
      return;
    }
    if (opponent) {
      const behind = Math.atan2(marioPos.x - opponent.x, marioPos.z - opponent.z);
      const delta = behind - this.yaw;
      this.yaw += Math.atan2(Math.sin(delta),Math.cos(delta)) * (1 - Math.exp(-8 * dt));
    } else this.followYaw(dt, marioYaw, moving);
    if (!this.initialized) {
      this.focus.set(marioPos.x, marioPos.y + 92, marioPos.z);
      this.snapDistances();
      this.initialized = true;
    }

    const target = new THREE.Vector3(marioPos.x, marioPos.y + 92, marioPos.z);
    if (battlefield) target.y = marioPos.y+125;
    const lookAhead = battlefield ? 0 : Math.min(90, Math.max(0, speed) * 2.2);
    target.x += Math.sin((toS16(marioYaw) * Math.PI * 2) / 65536) * lookAhead;
    target.z += Math.cos((toS16(marioYaw) * Math.PI * 2) / 65536) * lookAhead;
    if (opponent) {
      target.set(marioPos.x, marioPos.y + 95, marioPos.z).lerp(opponent, 0.35);
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
      this.focus.y + sp * distance,
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
      this.focus.y + (sp * distance) * scale,
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
    if (floor !== null) this.camera.position.y = Math.max(this.camera.position.y,floor+30);
    this.panDistance = 0;
    this.camera.lookAt(this.focus);
  }

  private radialYaw(mario: THREE.Vector3, speed: number): number {
    const old = this.lastRadialEye;
    const areaYaw = atan2s(mario.z+4608,mario.x-3072);
    const avoid = old && this.world ? radialWallYaw(mario,old,this.world) : undefined;
    let safeOffset: number | undefined;
    if (avoid !== undefined) {
      const tangent = avoid-areaYaw+0x4000<0 ? toS16(avoid+0x8000) : avoid;
      safeOffset = angleToRadians(clamp(toS16(tangent-areaYaw),-0x4aaa,0x4aaa));
    }
    const rotation = this.radialTarget-this.radialOffset;
    if (rotation) {
      if (safeOffset !== undefined && ((rotation>0 && safeOffset+angleToRadians(16)<this.radialOffset) ||
          (rotation<0 && safeOffset-angleToRadians(16)>this.radialOffset))) {
        this.radialOffset = this.radialTarget = safeOffset;
      } else {
        this.radialOffset += clamp(rotation,-angleToRadians(0x200),angleToRadians(0x200));
      }
    } else if (old) {
      if (safeOffset !== undefined) {
        const diff = toS16(this.radialOffset*32768/Math.PI-safeOffset*32768/Math.PI);
        this.radialOffset -= angleToRadians(Math.trunc(diff/10));
      } else {
        const rel = toS16(atan2s(old.z-mario.z,old.x-mario.x)-areaYaw);
        const goal = angleToRadians(clamp(rel,-0x2aaa,0x2aaa));
        const step = angleToRadians(Math.trunc(Math.abs(speed)*4));
        this.radialOffset += clamp(goal-this.radialOffset,-step,step);
      }
      this.radialTarget = this.radialOffset;
    }
    return toS16(areaYaw+this.radialOffset*32768/Math.PI);
  }

  private updateRadial(mario: THREE.Vector3, faceYaw: number, speed: number, action: number, pole?: SimObject | null) {
    const old = this.lastRadialEye;
    const yaw = this.radialYaw(mario,speed);
    const world = this.world;
    const floor = world?.findFloor(mario.x,mario.y+10,mario.z).height ?? mario.y;
    const height = (action & ACT_FLAG_METAL_WATER ? floor : Math.max(floor,world?.waterLevel(mario.x,mario.z) ?? floor))-mario.y;
    const onLowPole = (action & ACT_FLAG_ON_POLE) && pole && floor>=pole.pos[1] && mario.y<pole.pos[1]+0.7*pole.hitbox.height;
    const probe = world?.findFloor(mario.x+40*sins(yaw),mario.y,mario.z+40*coss(yaw),false,true);
    const rise = (probe?.height ?? mario.y)-mario.y;
    const slope = probe?.surface && probe.surface.type!==0x28 && rise>0 &&
      (probe.surface.normal[2]!==0 || rise>=100) ? atan2s(40,rise) : 0;
    const pitch = 0x5b0+slope+(this.pitch-0.31)*32768/Math.PI;
    const distance = this.distance+100;
    const eye = this.desired;
    eye.x = clamp(mario.x+distance*coss(pitch)*sins(yaw),-8000,8000);
    eye.z = clamp(mario.z+distance*coss(pitch)*coss(yaw),-8000,7050);
    const bound = onLowPole ? 1200 : 200;
    const goalHeight = mario.y+125+clamp(height,-bound,bound)+distance*sins(pitch);
    eye.y = this.radialHeight(mario,action,floor,goalHeight,old?.y ?? goalHeight);
    this.yaw = angleToRadians(atan2s(eye.z-mario.z,eye.x-mario.x));
    const target = this.direction.copy(mario);
    target.y += 125+clamp(0.9*height,-200,200);
    this.panAhead(mario,faceYaw,action,eye,target);
    this.lastRadialEye ??= new THREE.Vector3();
    this.lastRadialEye.copy(eye);
    if (this.leavingSummit) {
      this.radialReturn = new CameraTransition(this.camera.position,this.focus,target,60);
      this.renderedRadialEye.copy(this.camera.position);
      this.leavingSummit = false;
    }
    if (this.radialReturn?.left) {
      this.radialReturn.step(eye,target,eye,target,world);
      this.yaw = angleToRadians(atan2s(eye.z-target.z,eye.x-target.x));
    }
    if (old || this.radialReturn) {
      this.focus.x += (target.x-this.focus.x)*0.8;
      this.focus.y += (target.y-this.focus.y)*0.3;
      this.focus.z += (target.z-this.focus.z)*0.8;
      this.renderedRadialEye.lerp(eye,0.3);
    } else {
      this.focus.copy(target);
      this.renderedRadialEye.copy(eye);
      this.initialized = true;
    }
    const pos = this.renderedRadialEye;
    const below = world?.findFloor(pos.x,pos.y+20,pos.z,false,true);
    if (below?.surface) pos.y = Math.max(pos.y,below.height+100);
  }

  // set_camera_height raises the eye immediately over higher ground, but limits
  // its vertical speed over depressions and when Mario hangs from a ceiling.
  private radialHeight(mario: THREE.Vector3, action: number, floor: number, goalY: number, oldY: number): number {
    const eye = this.desired, world = this.world;
    const eyeFloor = world?.findFloor(eye.x,oldY+100,eye.z,false,true).height ?? floor;
    const eyeCeil = world?.findCeil(eye.x,oldY-50,eye.z,true);
    const marioCeil = world?.findCeil(mario.x,mario.y-10,mario.z,true);
    if (action & ACT_FLAG_HANGING && marioCeil) {
      const base = Math.max(floor,marioCeil.height-400);
      goalY = Math.max(base+(marioCeil.height-base)*0.4,mario.y-400);
      return oldY+clamp(goalY-oldY,-5,5);
    }
    const min = Math.max(floor,eyeFloor)+125;
    const y = goalY<min ? min : oldY+clamp(goalY-oldY,-20,20);
    if (eyeCeil?.surface && (floor+125<eyeCeil.height-125 ||
          (marioCeil?.surface && marioCeil.height>eyeCeil.height-125))) return Math.min(y,eyeCeil.height-125);
    return y;
  }

  private panAhead(marioPos: THREE.Vector3, marioYaw: number, action: number, eye: THREE.Vector3, target: THREE.Vector3): void {
    const cameraYaw = atan2s(marioPos.z-eye.z, marioPos.x-eye.x);
    const side = sins(marioYaw)*coss(cameraYaw)-coss(marioYaw)*sins(cameraYaw);
    const reverse = action === ACT_LONG_JUMP || ((action & ACT_FLAG_ON_POLE) && action !== ACT_TOP_OF_POLE);
    const sleeping = action === ACT_SLEEPING || action === ACT_START_SLEEPING;
    const pan = sleeping ? 0 : side*sins(0xc00)*eye.distanceTo(marioPos)*(reverse ? -1 : 1);
    this.panDistance += (pan-this.panDistance)*0.025;
    target.x += this.panDistance*coss(cameraYaw);
    target.z -= this.panDistance*sins(cameraYaw);
  }

  private sideTicks = 0;
  private pitchTicks = 0;

  // mode_behind_mario, one fixed tick after Mario advances his cannon trajectory.
  followCannonShot(pos: Vec3, face: Vec3, side: number, vertical: number, launch: boolean): void {
    if (launch) this.panDistance = this.pitchTicks = this.sideTicks = 0;
    const mario = this.desired.fromArray(pos);
    this.focus.copy(mario);
    this.focus.y += 125;
    const eye = this.camera.position;
    const dx = eye.x-this.focus.x, dy = eye.y-this.focus.y, dz = eye.z-this.focus.z;
    let distance = Math.min(800,Math.hypot(dx,dy,dz));
    let yaw = atan2s(dz,dx), pitch = atan2s(Math.hypot(dx,dz),dy);
    let yawDivisor = clamp(32-Math.trunc(Math.abs(pitch)/0x200),1,32);
    let yawGoal = face[1]+0x8000, pitchGoal = -face[0], pitchStep = 128;
    if (this.sideTicks > 0) { this.sideTicks--; yawDivisor = 8; }
    if (this.pitchTicks > 0) { this.pitchTicks--; pitchGoal = 0; pitchStep = 0x800; }
    if (side) {
      distance = Math.min(800,distance+5);
      yawGoal += side*0x3ff8;
      this.sideTicks = 30;
      yawDivisor = 2;
    }
    if (vertical) {
      distance = Math.min(800,distance+5);
      pitchGoal = -vertical*0x3000;
      this.pitchTicks = 30;
      pitchStep = 0x800;
    }
    yaw = toS16(yaw-Math.trunc(toS16(yaw-yawGoal)/yawDivisor));
    pitch = toS16(pitch+clamp(toS16(pitchGoal-pitch),-pitchStep,pitchStep));
    distance = Math.max(300,distance);
    eye.x = this.focus.x+distance*coss(pitch)*sins(yaw);
    eye.z = this.focus.z+distance*coss(pitch)*coss(yaw);
    eye.y += clamp(this.focus.y+distance*sins(pitch)-eye.y,-50,50);
    if (this.world) {
      const point: Vec3 = eye.toArray();
      this.world.f32FindWallCollision(point,0,100,true);
      eye.fromArray(point);
      const floor = this.world.findFloor(eye.x,eye.y+50,eye.z,false,true);
      const ceil = this.world.findCeil(eye.x,eye.y-50,eye.z,true);
      const low = floor.surface ? floor.height+125 : -Infinity;
      const high = ceil.surface ? ceil.height-125 : Infinity;
      if (eye.y <= low && eye.y < high) eye.y = low;
      if (eye.y > low && eye.y >= high) eye.y = high;
      if (eye.y <= low && eye.y >= high) eye.y = (low+high)/2;
    }
    this.direction.copy(eye).sub(this.focus).clampLength(0,800);
    eye.copy(this.focus).add(this.direction);
    this.yaw = angleToRadians(yaw);
    this.panAhead(mario,face[1],0,eye,this.direction.copy(this.focus));
    this.camera.lookAt(this.direction);
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
