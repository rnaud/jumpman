import * as THREE from 'three';
import { atan2s, clamp, coss, sins, toS16 } from '../sim/math';
import type { SimObject } from '../sim/object';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { A_BUTTON, B_BUTTON, ACT_READING_NPC_DIALOG, type ControllerSnapshot } from '../sim/state';
import type { Cannons } from './cannons';

// US dialog 004, 105, 047 and 106, split at their native page boundaries.
const battlefield = [
  ["We're peace-loving Bob-ombs, so we don't use cannons.", "But if you'd like to blast off, we don't mind. Help yourself.",
    "We'll prepare all of the cannons in this course for you to use. Bon Voyage!"],
  ["Ready for blastoff! Come on, hop into the cannon!", "You can reach the Star on the floating island by using the four cannons.",
    "Use the Control Stick to aim, then press [A] to fire.", "If you're handy, you can grab on to trees or poles to land."],
];
const otherCourses = [["Hi! I'll prepare the cannon for you!"], [battlefield[1][0]]];

function polar(from: THREE.Vector3, to: THREE.Vector3) {
  const dx = to.x-from.x, dy = to.y-from.y, dz = to.z-from.z;
  return {dist:Math.hypot(dx,dy,dz), pitch:atan2s(Math.hypot(dx,dz),dy), yaw:atan2s(dz,dx)};
}

function offset(to: THREE.Vector3, from: THREE.Vector3, dist: number, pitch: number, yaw: number) {
  to.set(from.x+dist*coss(pitch)*sins(yaw),from.y+dist*sins(pitch),from.z+dist*coss(pitch)*coss(yaw));
}

export class CannonOperator {
  private page = 0;
  private stage: 'turn' | 'intro' | 'opening' | 'ready' = 'turn';
  private timer = 0;
  private readonly lid = new THREE.Vector3();
  private readonly focus = new THREE.Vector3();
  private readonly originalRotation = new THREE.Quaternion();
  private readonly originalEye = new THREE.Vector3();
  private readonly originalFocus = new THREE.Vector3();
  private readonly pages: string[][];

  constructor(readonly buddy: SimObject, isBattlefield: boolean, private readonly unlocked: boolean) {
    this.pages = isBattlefield ? battlefield : otherCourses;
  }

  get opening(): boolean { return this.stage === 'opening'; }
  get text(): string | undefined {
    if (this.stage === 'intro') return this.pages[0][this.page];
    if (this.stage === 'ready') return this.pages[1][this.page];
  }

  step(m: MarioRuntime, controls: ControllerSnapshot, camera: THREE.Camera, cannons: Cannons, saveUnlock: () => void, playDoor: (pos: THREE.Vector3) => void): boolean {
    if (this.stage === 'turn') {
      this.buddy.turnTowardAngle(this.buddy.angleToMario,0x1000);
      this.buddy.faceYaw = this.buddy.moveYaw;
      if (toS16(this.buddy.moveYaw-this.buddy.angleToMario) === 0) {
        if (m.action !== ACT_READING_NPC_DIALOG) {
          m.usedObj = this.buddy;
          m.setAction(ACT_READING_NPC_DIALOG,1);
        } else if (m.actionState === 8) this.stage = this.unlocked ? 'ready' : 'intro';
      }
    } else if (this.opening) {
      cannons.step(m);
      if (this.timer === 61) playDoor(this.lid);
      this.updateCamera(camera);
      if (++this.timer === 171) {
        camera.position.copy(this.originalEye);
        camera.quaternion.copy(this.originalRotation);
        this.stage = 'ready';
      }
    } else if (controls.buttonPressed & (A_BUTTON | B_BUTTON)) {
      if (++this.page === this.pages[this.stage === 'intro' ? 0 : 1].length) {
        this.page = 0;
        if (this.stage === 'ready') {
          m.actionState = 9;
          return true;
        }
        saveUnlock();
        const nearest = cannons.unlock(this.buddy.pos);
        if (nearest) {
          this.stage = 'opening';
          this.lid.fromArray(nearest);
          this.originalEye.copy(camera.position);
          this.originalRotation.copy(camera.quaternion);
          camera.getWorldDirection(this.originalFocus).multiplyScalar(camera.position.distanceTo(new THREE.Vector3(...m.pos))).add(camera.position);
          this.focus.copy(this.originalFocus);
        } else this.stage = 'ready';
      }
    }
    return false;
  }

  private updateCamera(camera: THREE.Camera): void {
    const eye = camera.position, t = this.timer;
    if (t >= 141) {
      const factor = eye.distanceTo(this.originalEye) >= 8000 ? 1 : 1/Math.max(1,170-t);
      eye.lerp(this.originalEye,factor);
      this.focus.lerp(this.originalFocus,factor);
      camera.lookAt(this.focus);
      return;
    }
    const orbit = polar(this.lid,eye);
    if (orbit.dist > 8000) {
      orbit.dist = 1200;
      orbit.pitch = 0x2000;
      this.focus.copy(this.lid);
    } else {
      const current = polar(eye,this.focus), target = polar(eye,this.lid);
      offset(this.focus,eye,current.dist+(target.dist-current.dist)*0.1,
        current.pitch-Math.trunc(toS16(current.pitch-target.pitch)/15),
        current.yaw-Math.trunc(toS16(current.yaw-target.yaw)/15));
    }
    offset(eye,this.lid,orbit.dist+(300-orbit.dist)*0.05,
      orbit.pitch-Math.trunc(toS16(orbit.pitch-0x2000)/32),orbit.yaw);
    const aim = polar(eye,this.focus);
    const target = new THREE.Vector3();
    offset(target,eye,aim.dist,clamp(aim.pitch,-0x3000,0x3000),aim.yaw+Math.min(0x400,17*t));
    camera.lookAt(target);
  }
}
