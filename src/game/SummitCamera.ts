import { Vector3 } from 'three';
import { CELL_HEIGHT_LIMIT, type CollisionWorld } from '../sim/collision';
import { angleToRadians, atan2s, clamp, coss, sins, toS16 } from '../sim/math';
import { CameraTransition } from './CameraTransition';

/** The Battlefield branch of camera.c's update_boss_fight_camera, stepped at 30 Hz. */
export class SummitCamera {
  private readonly goalFocus = new Vector3();
  private readonly goalEye = new Vector3();
  private readonly nextEye = new Vector3();
  private readonly nextFocus = new Vector3();
  private readonly eye: Vector3;
  private readonly focus: Vector3;
  private transition?: CameraTransition;
  private entering = true;
  private ticks = 0;
  private offsetYaw = 0;
  private sideFrames = 0;
  private sideInput = 0;
  private zoomDist = 0;
  private zoomPitch = 0;
  zoomedOut = false;
  yaw: number;

  constructor(eye: Vector3, focus: Vector3, yaw: number) {
    this.eye = eye.clone();
    this.focus = focus.clone();
    this.yaw = yaw;
  }

  orbit(side: number, vertical: number) {
    const direction = Math.sign(side);
    if (direction && direction !== this.sideInput) this.sideFrames = direction*16;
    this.sideInput = direction;
    if (vertical) this.zoomedOut = vertical > 0;
  }

  drag(dx: number, dy: number) {
    this.offsetYaw = toS16(this.offsetYaw-dx*0.0042*32768/Math.PI);
    if (dy) this.zoomedOut = dy > 0;
  }

  update(dt: number, mario: Vector3, boss: Vector3, held: boolean, yaw: number, pitch: number, spin: number,
    outEye: Vector3, outFocus: Vector3, world?: CollisionWorld) {
    this.ticks += dt*30;
    while (this.ticks >= 1) {
      this.ticks--;
      const distance = clamp(mario.distanceTo(boss)*1.6,800,5000);
      const focus = this.goalFocus;
      focus.copy(boss);
      if (held) {
        // offset_rotated flips local Z before applying Mario's yaw.
        const forward = 125*coss(pitch)-150*sins(pitch);
        focus.set(mario.x+forward*sins(yaw),mario.y-125*sins(pitch)-150*coss(pitch),mario.z+forward*coss(yaw));
      }
      focus.lerp(mario,0.5);
      focus.y += 100 + (held ? 300*sins(Math.abs(spin)) : 0);
      const bearing = this.offsetYaw+0x2000;
      const eye = this.goalEye;
      const horizontal = distance*coss(0x1000);
      eye.set(focus.x+horizontal*sins(bearing),0,focus.z+horizontal*coss(bearing));
      const center = world?.findFloor(3072,CELL_HEIGHT_LIMIT,-4608,false,true);
      const plane = center?.surface;
      eye.y = plane ? 550-(plane.normal[0]*eye.x+plane.normal[2]*eye.z+plane.originOffset)/plane.normal[1]
        : (center?.height ?? mario.y+550);

      const side = Math.sign(this.sideFrames);
      this.offsetYaw = toS16(this.offsetYaw+side*0x200);
      this.sideFrames -= side;
      const target = this.zoomDist>distance || this.zoomedOut ? distance : 0;
      this.zoomDist += clamp(target-this.zoomDist,-30,30);
      this.zoomPitch = clamp(this.zoomPitch+(this.zoomedOut ? 1 : -1)*Math.trunc(0x1800/13),0,0x1800);
      const zoomAngle = this.zoomPitch+0x1000;
      eye.x += this.zoomDist*coss(zoomAngle)*sins(bearing);
      eye.y += this.zoomDist*sins(zoomAngle);
      eye.z += this.zoomDist*coss(zoomAngle)*coss(bearing);

      if (this.entering) {
        this.transition = new CameraTransition(this.eye,this.focus,focus,15);
        this.entering = false;
      }
      if (this.transition?.left) {
        this.transition.step(eye,focus,this.nextEye,this.nextFocus,world);
        this.yaw = angleToRadians(atan2s(this.nextEye.z-this.nextFocus.z,this.nextEye.x-this.nextFocus.x));
      } else {
        this.nextEye.copy(eye);
        this.nextFocus.copy(focus);
        this.yaw = angleToRadians(bearing);
      }
      this.eye.lerp(this.nextEye,0.3);
      this.focus.x += 0.8*(this.nextFocus.x-this.focus.x);
      this.focus.z += 0.8*(this.nextFocus.z-this.focus.z);
      this.focus.y += 0.3*(this.nextFocus.y-this.focus.y);
    }
    outFocus.copy(this.focus);
    outEye.copy(this.eye);
    const floor = world?.findFloor(outEye.x,outEye.y+20,outEye.z,false,true);
    if (floor?.surface) outEye.y = Math.max(outEye.y,floor.height+100);
  }
}
