import { Vector3 } from 'three';
import type { CollisionWorld } from '../sim/collision';
import { atan2s, clamp, coss, sins, toS16, type Vec3 } from '../sim/math';

export function polar(from: Vector3, to: Vector3) {
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

/** next_lakitu_state transitions around the new focus in spherical coordinates. */
export class CameraTransition {
  private eye: ReturnType<typeof polar>;
  private focus: ReturnType<typeof polar>;

  constructor(oldEye: Vector3, oldFocus: Vector3, goalFocus: Vector3, public left: number) {
    this.eye = polar(goalFocus,oldEye);
    this.focus = polar(goalFocus,oldFocus);
  }

  step(goalEye: Vector3, goalFocus: Vector3, outEye: Vector3, outFocus: Vector3, world?: CollisionWorld) {
    const {eye,focus,left} = this;
    const goal = polar(goalFocus,goalEye);
    eye.dist += (goal.dist-eye.dist)/left;
    eye.pitch = advanceAngle(eye.pitch,goal.pitch,(goal.pitch-eye.pitch)/left);
    eye.yaw = advanceAngle(eye.yaw,goal.yaw,(goal.yaw-eye.yaw)/left);
    const reverse = polar(goalEye,goalFocus);
    focus.dist -= focus.dist/left;
    focus.pitch = advanceAngle(focus.pitch,reverse.pitch,focus.pitch/left);
    focus.yaw = advanceAngle(focus.yaw,reverse.yaw,focus.yaw/left);
    place(outEye,goalFocus,eye);
    place(outFocus,goalFocus,focus);
    if (world) {
      const pos: Vec3 = outEye.toArray();
      const floor = world.findFloor(...pos,false,true);
      if (floor.surface) pos[1] = Math.max(pos[1],floor.height+125);
      world.f32FindWallCollision(pos,0,100,true);
      outEye.fromArray(pos);
    }
    this.left--;
  }
}
