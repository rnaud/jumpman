import * as THREE from 'three';
import { angleToRadians, atan2s, radiansToAngle, sins, clamp, toS16, type Vec3 } from '../sim/math';
import type { CollisionWorld, Surface } from '../sim/collision';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { ACT_FLAG_AIR } from '../sim/state';
import type { CollisionData } from './nativeCollision';

/** A rigid transform shared by the display, the camera blocker, and Mario's surfaces. */
export class MovingPlatform {
  readonly blocker: THREE.Mesh;
  private readonly home: THREE.Vector3;
  private readonly baseYaw: number;
  private readonly local = new THREE.Vector3();
  private surfaces = new Set<Surface>();
  private waypoint = 1;
  private moveYaw = 0;
  private pitchVelocity = 0;
  private pitch = 0;
  private tick = 0;
  private phase = 0;
  private speed = 0;
  private readonly initialTimer: number;
  private readonly tower: boolean;

  constructor(private readonly model: THREE.Object3D, private readonly data: CollisionData,
      private readonly kind: string, private readonly param: number, private readonly track?: Vec3[]) {
    this.tower = kind.startsWith('tower_');
    this.home = model.position.clone();
    this.baseYaw = model.rotation.y;
    this.initialTimer = ['sliding_platform', 'small_bomp', 'large_bomp'].includes(kind) ? Math.floor(Math.random() * 100) : 0;
    if (kind === 'sliding_platform') this.home.x += 2;
    this.resetPose();
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.Float32BufferAttribute(data.vertices.flat(), 3));
    geometry.setIndex(data.triangles.flatMap(([a,b,c]) => [a,b,c]));
    this.blocker = new THREE.Mesh(geometry, new THREE.MeshBasicMaterial({ side: THREE.DoubleSide, wireframe: true }));
    this.blocker.userData.nativeCollision = true;
    this.blocker.visible = false;
    this.syncTransform();
  }

  private syncTransform(): void {
    this.blocker.position.copy(this.model.position);
    this.blocker.quaternion.copy(this.model.quaternion);
    this.blocker.updateMatrixWorld(true);
  }

  refresh(world: CollisionWorld): void {
    world.removeSurfaces(this.surfaces);
    this.surfaces.clear();
    if (this.tower && !this.model.visible) return;
    const vertices: Vec3[] = this.data.vertices.map(v =>
      this.local.fromArray(v).applyMatrix4(this.blocker.matrixWorld).toArray() as Vec3);
    for (const [a,b,c,type,force] of this.data.triangles) {
      const surface = world.addTriangle(vertices[a], vertices[b], vertices[c], { type, force });
      if (surface) this.surfaces.add(surface);
    }
  }

  private resetPose(): void {
    this.waypoint = 1;
    this.moveYaw = 0;
    this.pitch = this.pitchVelocity = this.phase = this.speed = 0;
    if (this.tower) this.model.visible = false;
    this.tick = this.initialTimer;
    this.model.position.copy(this.home);
    const yaw = ['sliding_platform', 'small_bomp'].includes(this.kind) ? this.baseYaw - Math.PI / 2 : this.baseYaw;
    this.model.rotation.set(0, yaw, 0, 'YXZ');
  }

  reset(world: CollisionWorld): void {
    this.resetPose();
    this.syncTransform();
    this.refresh(world);
  }

  private stepRopeway(m: MarioRuntime, riding: boolean, track: Vec3[]): void {
    const pos = this.model.position;
    if (this.phase === 0) {
      if (!riding) this.tick = 0;
      else if (this.tick > 20) this.phase = 1;
    } else if (this.waypoint === track.length) {
      this.speed = 0;
      this.waypoint = 1;
      this.phase = 0;
    } else {
      this.speed = Math.min(10, Math.fround(this.speed + 0.1));
      let remaining = this.speed;
      while (remaining > 0 && this.waypoint < track.length) {
        const [x, y, z] = track[this.waypoint];
        const dx = x - pos.x, dy = y - pos.y, dz = z - pos.z;
        const length = Math.hypot(dx, dy, dz);
        if (length > 0) this.moveYaw = atan2s(dz, dx);
        if (length <= remaining) {
          pos.set(x, y, z);
          this.waypoint++;
          remaining -= length;
        } else {
          const fraction = remaining / length;
          pos.set(pos.x + fraction * dx, pos.y + fraction * dy, pos.z + fraction * dz);
          break;
        }
      }
    }

    // platform_on_track_rock_ski_lift advances roll once before oscillate_toward.
    const dx = m.pos[0] - pos.x, dz = m.pos[2] - pos.z;
    const distance = Math.hypot(dx, m.pos[1] - pos.y, dz);
    const lever = distance * sins(atan2s(dz, dx) - radiansToAngle(this.baseYaw));
    const target = riding ? Math.trunc(-50 * this.speed * sins(this.moveYaw) + Math.trunc(-4 * lever)) : 0;
    this.pitch += Math.trunc(this.pitchVelocity);
    const previous = this.pitch;
    this.pitch += Math.trunc(this.pitchVelocity);
    if (this.pitch === target || ((this.pitch-target)*(previous-target) < 0 && Math.abs(this.pitchVelocity) < 5)) {
      this.pitch = target;
      this.pitchVelocity = 0;
    } else {
      const accel = this.pitch >= target ? -6 : 6;
      this.pitchVelocity = clamp(this.pitchVelocity + accel * (this.pitchVelocity * accel < 0 ? 1.5 : 1), -100, 100);
    }
    this.model.rotation.z = angleToRadians(this.pitch);
  }

  step(m: MarioRuntime): 'moving' | 'stop' | undefined {
    let liftSound: 'moving' | 'stop' | undefined;
    if (this.tower) {
      const threshold = this.home.y - 100*this.param - 1000;
      if (this.model.visible && m.pos[1] < threshold) this.reset(m.world);
      if (!this.model.visible && m.pos[1] <= threshold) return;
      this.model.visible = true;
    }
    const riding = m.floor && this.surfaces.has(m.floor) && !(m.action & ACT_FLAG_AIR)
      && Math.abs(m.pos[1] - m.floorHeight) < 4;
    const rider = riding ? this.blocker.worldToLocal(this.local.fromArray(m.pos)).clone() : null;
    const yaw = this.model.rotation.y;
    const oldPhase = this.phase;
    switch (this.kind) {
      case 'ropeway_lift':
        if (this.phase === 1) liftSound = this.waypoint === this.track?.length ? 'stop' : 'moving';
        if (this.track) this.stepRopeway(m, !!riding, this.track);
        break;
      case 'tower_slide': {
        const velocity = this.phase === 0 ? -3 : 3;
        this.model.position.x += velocity*Math.sin(yaw);
        this.model.position.z += velocity*Math.cos(yaw);
        if (this.tick > Math.trunc(380/3)) this.phase = 1-this.phase;
        break;
      }
      case 'tower_elevator':
        if (this.phase === 0 && riding) this.phase = 1;
        else if (this.phase === 1 || this.phase === 3) {
          if (this.tick > 140) this.phase = this.phase === 1 ? 2 : 0;
          else this.model.position.y += this.phase === 1 ? 5 : -5;
        } else if (this.phase === 2 && this.tick > 60) this.phase = 3;
        break;
      case 'seesaw_platform': {
        this.pitch += Math.trunc(this.pitchVelocity);
        if (rider) {
          const lever = (m.pos[0] - this.home.x) * Math.sin(yaw) + (m.pos[2] - this.home.z) * Math.cos(yaw);
          this.pitchVelocity = clamp(this.pitchVelocity + lever * (this.pitchVelocity * lever < 0 ? 0.04 : 0.02), -50, 50);
        } else {
          const before = this.pitch;
          this.pitch += Math.trunc(this.pitchVelocity);
          if (this.pitch === 0 || (before * this.pitch < 0 && Math.abs(this.pitchVelocity) < 6)) {
            this.pitch = this.pitchVelocity = 0;
          } else {
            const accel = this.pitch >= 0 ? -3 : 3;
            this.pitchVelocity += accel * (this.pitchVelocity * accel < 0 ? 3 : 1);
          }
        }
        this.model.rotation.set(angleToRadians(this.pitch), yaw, 0, 'YXZ');
        break;
      }
      case 'rotating_wooden_platform':
        if (this.phase === 0) {
          if (this.tick > 60) this.phase = 1;
        } else {
          this.model.rotation.y += angleToRadians(0x100);
          if (this.tick > 126) this.phase = 0;
        }
        break;
      case 'rotating_platform':
        this.model.rotation.y += angleToRadians(this.param << 4);
        break;
      case 'merry_go_round':
        this.model.rotation.y += angleToRadians(0x60);
        break;
      case 'mesh_elevator':
        this.model.position.y = this.home.y + Math.sin(this.tick * 0.025) * 420;
        break;
      case 'moving_bookshelf':
        this.model.position.x = this.home.x + Math.sin(this.tick * 0.018) * 240;
        break;
      case 'tilting_trap_platform':
        this.model.rotation.set(Math.sin(this.tick * 0.03) * 0.2, yaw, 0, 'YXZ');
        break;
      case 'sliding_platform': {
        const speed = [0, 10, 15, 20][this.param] ?? 15;
        if (this.phase === 0 && this.tick > 100) {
          this.phase = 1; this.speed = speed;
        } else if (this.phase === 1) {
          if (this.tick >= 500 / speed) { this.speed = 0; this.model.position.x = this.home.x + 510; }
          if (this.tick === 60) { this.phase = 2; this.speed = -speed; }
        } else if (this.phase === 2) {
          if (this.tick >= 500 / speed) { this.speed = 0; this.model.position.x = this.home.x; }
          if (this.tick === 90) { this.phase = 1; this.speed = speed; }
        }
        this.model.position.x += this.speed;
        break;
      }
      case 'small_bomp':
      case 'large_bomp':
        if (this.phase === 0 && this.tick > 100) { this.phase = 1; this.speed = 30; }
        else if (this.phase === 1) {
          if (this.model.position.x > 3450) { this.model.position.x = 3450; this.speed = 0; }
          if (this.tick === 15) { this.phase = 2; this.speed = this.kind === 'large_bomp' ? 10 : 40; }
        } else if (this.phase === 2) {
          if (this.model.position.x > 3830) { this.model.position.x = 3830; this.speed = 0; }
          if (this.tick === 60) { this.phase = 3; this.speed = -10; }
        } else if (this.phase === 3) {
          if (this.model.position.x < 3330) { this.model.position.x = 3330; this.speed = 0; }
          if (this.tick === 90) { this.phase = 1; this.speed = 25; }
        }
        this.model.position.x += this.speed;
        break;
    }
    this.tick = oldPhase === this.phase ? this.tick + 1 : 0;
    this.syncTransform();
    if (rider) {
      this.blocker.localToWorld(rider);
      m.pos = rider.toArray() as Vec3;
      m.faceAngle[1] = toS16(m.faceAngle[1] + Math.round((this.model.rotation.y - yaw) * 32768 / Math.PI));
    }
    this.refresh(m.world);
    return liftSound;
  }
}
