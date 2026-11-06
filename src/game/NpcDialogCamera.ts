import { PerspectiveCamera, Vector3 } from 'three';
import type { CollisionWorld } from '../sim/collision';
import { atan2s, clamp, coss, sins, toS16, type Vec3 } from '../sim/math';

function polar(from: Vector3, to: Vector3) {
  const dx = to.x-from.x, dy = to.y-from.y, dz = to.z-from.z;
  return {dist: Math.hypot(dx,dy,dz), pitch: atan2s(Math.hypot(dx,dz),dy), yaw: atan2s(dz,dx)};
}

function place(out: Vector3, origin: Vector3, {dist,pitch,yaw}: ReturnType<typeof polar>) {
  out.set(origin.x+dist*coss(pitch)*sins(yaw), origin.y+dist*sins(pitch), origin.z+dist*coss(pitch)*coss(yaw));
}

function advanceAngle(current: number, target: number, step: number) {
  const limit = Math.abs(toS16(Math.trunc(step)));
  return toS16(current+clamp(toS16(target-current),-limit,limit));
}

/** camera.c: cutscene_dialog_start, move_mario_shoulder and next_lakitu_state. */
export class NpcDialogCamera {
  readonly savedEye: Vector3;
  private readonly focus: Vector3;
  private readonly pivot: Vector3;
  private readonly npc: Vector3;
  private readonly yaw: number;
  private readonly target = new Vector3();
  private transition?: {eye: ReturnType<typeof polar>; focus: ReturnType<typeof polar>; left: number};
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
    this.transition = {eye:polar(this.savedFocus,camera.position),focus:polar(this.savedFocus,this.focus),left:15};
  }

  get returnFrames() { return this.transition?.left; }

  stepReturn(camera: PerspectiveCamera, world: CollisionWorld) {
    const transition = this.transition!;
    const {eye,focus,left} = transition;
    const goal = polar(this.savedFocus,this.savedEye);
    eye.dist += (goal.dist-eye.dist)/left;
    eye.pitch = advanceAngle(eye.pitch,goal.pitch,(goal.pitch-eye.pitch)/left);
    eye.yaw = advanceAngle(eye.yaw,goal.yaw,(goal.yaw-eye.yaw)/left);
    const reverse = polar(this.savedEye,this.savedFocus);
    focus.dist -= focus.dist/left;
    focus.pitch = advanceAngle(focus.pitch,reverse.pitch,focus.pitch/left);
    focus.yaw = advanceAngle(focus.yaw,reverse.yaw,focus.yaw/left);
    place(camera.position,this.savedFocus,eye);
    place(this.focus,this.savedFocus,focus);
    const pos: Vec3 = camera.position.toArray();
    const floor = world.findFloor(...pos,false,true);
    if (floor.surface) pos[1] = Math.max(pos[1],floor.height+125);
    world.f32FindWallCollision(pos,0,100,true);
    camera.position.fromArray(pos);
    camera.lookAt(this.focus);
    transition.left--;
  }
}
