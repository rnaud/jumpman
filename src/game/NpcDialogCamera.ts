import { PerspectiveCamera, Vector3 } from 'three';
import type { CollisionWorld } from '../sim/collision';
import { atan2s, coss, sins, toS16, type Vec3 } from '../sim/math';

import { CameraTransition, polar } from './CameraTransition';

/** camera.c: cutscene_dialog_start, move_mario_shoulder and next_lakitu_state. */
export class NpcDialogCamera {
  readonly savedEye: Vector3;
  private readonly focus: Vector3;
  private readonly pivot: Vector3;
  private readonly npc: Vector3;
  private readonly yaw: number;
  private readonly target = new Vector3();
  private transition?: CameraTransition;
  frame = 0;

  constructor(camera: PerspectiveCamera, private readonly savedFocus: Vector3, mario: Vec3, npc: Vec3, height: number) {
    this.savedEye = camera.position.clone();
    this.focus = savedFocus.clone();
    this.pivot = new Vector3(...mario);
    this.pivot.y += 125;
    this.npc = new Vector3(...npc);
    this.npc.y += height+200;
    const toward = polar(this.pivot,this.npc).yaw;
    this.yaw = toS16(toward+(toS16(polar(this.pivot,this.savedEye).yaw-toward)<0 ? -0x6000 : 0x6000));
  }

  step(camera: PerspectiveCamera, mario: Vec3) {
    this.target.fromArray(mario).lerp(this.npc,0.3);
    this.target.y += (this.npc.y-this.target.y)*0.1;
    this.focus.lerp(this.target,0.2);
    const eye = camera.position;
    const dx = eye.x-this.pivot.x, dz = eye.z-this.pivot.z;
    const yaw = atan2s(dz,dx);
    const next = yaw-Math.trunc(toS16(yaw-this.yaw)/16);
    const dist = 0.95*Math.hypot(dx,dz)+9;
    eye.x = this.pivot.x+dist*sins(next);
    eye.z = this.pivot.z+dist*coss(next);
    eye.y += (this.pivot.y+100*sins(polar(this.npc,this.pivot).pitch)-eye.y)*0.05;
    camera.lookAt(this.focus);
    this.frame++;
  }

  beginReturn(camera: PerspectiveCamera) {
    this.transition = new CameraTransition(camera.position,this.focus,this.savedFocus,15);
  }

  get returnFrames() { return this.transition?.left; }

  stepReturn(camera: PerspectiveCamera, world: CollisionWorld) {
    this.transition!.step(this.savedEye,this.savedFocus,camera.position,this.focus,world);
    camera.lookAt(this.focus);
  }
}
