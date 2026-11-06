import * as THREE from 'three';
import { angleToRadians, atan2s, sins, toS16, type Vec3 } from '../sim/math';

// SHAKE_POS_BOWLING_BALL in camera.c. The distance falloff starts at 2000,
// which for this preset is also the cutoff. Only the view-angle penalty applies.
export class CameraPitchShake {
  private magnitude = 0;
  private phase = 0;
  private readonly forward = new THREE.Vector3();
  radians = 0;

  requestBowlingBall(camera: THREE.Camera, pos: Vec3): void {
    const dx = pos[0]-camera.position.x, dy = pos[1]-camera.position.y, dz = pos[2]-camera.position.z;
    if (dx*dx + dy*dy + dz*dz >= 2000*2000) return;
    const view = camera.getWorldDirection(this.forward);
    const pitch = toS16(atan2s(Math.hypot(dx,dz),dy) - atan2s(Math.hypot(view.x,view.z),view.y));
    const yaw = toS16(atan2s(dz,dx) - atan2s(view.z,view.x));
    const offAxis = pitch < -0x1800 || pitch > 0x400 || Math.abs(yaw) > 0x1800;
    this.magnitude = Math.max(this.magnitude, offAxis ? 0x14 : 0x28);
  }

  requestCannonRaise(): void { this.magnitude = Math.max(this.magnitude,0x60); }

  step(): void {
    this.radians = angleToRadians(this.magnitude * sins(this.phase));
    this.magnitude = Math.max(0, this.magnitude - 8);
    this.phase = this.magnitude ? toS16(this.phase + 0x4000) : 0;
  }
}
