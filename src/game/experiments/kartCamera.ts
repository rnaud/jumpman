/**
 * The Mario Kart 64 one-player chase camera, engine-free. Browser twin of native/src/experiments/kart_camera.rs.
 *
 * Port of n64decomp/mk64 src/camera.c: camera_init (1P offsets), func_8001E45C (heading follow and drift swing),
 * func_8001CCEC (the position and look-at smoothing, mushroom push and wall-bump kick) and func_8001F394 /
 * func_80014EE4 (field of view). Everything is in MK64 course units and s16 angles and runs at the game's 60 Hz,
 * so callers step it twice per 30 Hz sim tick and interpolate for rendering.
 *
 * Numbers that matter (course units; the world is these x 8):
 *   eye    = kart + (0, 9.5, -50) turned by the camera heading      (-50 = behind)
 *   target = kart + (0, 0, 70) turned by the camera heading         (70 = ahead)
 *   eye follows with 0.4 horizontally and 0.15 vertically per frame, the target with 0.4 / 0.5
 *   the heading chases the kart's rotation by 165..500 s16 per frame (more when steering harder), swings 12 degrees
 *   towards the inside of a drift, and stops following while the kart spins out
 *   vertical field of view 40 degrees (+8 for a mushroom, +18 while spinning out), eased 1 degree per frame
 * Not ported: the wall/ground collision push-out of the camera, the Lakitu retrieval view, split-screen layouts
 * and the intro fly-by modes.
 */
import type { KartPhysics } from './kartPhysics';

export const CAMERA_HEIGHT = 9.5;
export const CAMERA_BACK = -50;
export const CAMERA_AHEAD = 70;
export const FOLLOW_XZ = 0.4;
export const FOLLOW_EYE_Y = 0.15;
export const FOLLOW_LOOK_Y = 0.5;
export const BASE_FOV = 40;
export const MUSHROOM_FOV = 8;
export const SPINOUT_FOV = 18;
/** The port's kart origin sits this far below the decomp's player->pos (BOUNDING_BOX). */
export const ORIGIN_LIFT = 5.5;
const DEG = 182;
const s16 = (v: number) => (Math.trunc(v) << 16) >> 16;
// move_f32_towards / move_s16_towards
const towards = (value: number, target: number, rate: number) => {
  const v = value - (value - target) * rate;
  return Math.abs(v) < 0.001 ? 0 : v;
};
const towardsS16 = (value: number, target: number, rate: number) => s16(value - (value - target) * rate);

/** adjust_angle: move `angle` towards `target` by at most `step` s16 units. */
export function adjustAngle(angle: number, target: number, step: number): number {
  const diff = s16(target - angle);
  const s = Math.abs(step);
  if (diff > 0) {
    const left = diff - s;
    return left >= 0 ? s16(target - left) : target;
  }
  const left = diff + s;
  return left <= 0 ? s16(target - left) : target;
}

/** What the camera needs to know about the kart each 60 Hz frame. */
export interface CameraKart {
  /** Position in course units (the port's origin; ORIGIN_LIFT is added here). */
  pos: [number, number, number];
  /** s16 rotation[1]. */
  rotation: number;
  drifting: boolean;
  driftOutside: boolean;
  /** unk_078: signed turning rate. */
  turn: number;
  kmh: number;
  hop: boolean;
  /** Hit a wall this frame. */
  bumped: boolean;
  mushroom: boolean;
  spinout: boolean;
}

export const cameraKart = (k: KartPhysics, mushroom: boolean, spinout: boolean): CameraKart => ({
  pos: k.pos, rotation: k.rotation, drifting: k.drifting, driftOutside: k.driftOutside, turn: k.unk078, kmh: k.kmh(),
  hop: k.hop, bumped: k.bumped, mushroom, spinout,
});

export interface CameraPose {
  /** Eye and look-at in course units. */
  eye: [number, number, number];
  look: [number, number, number];
  /** Vertical field of view, degrees. */
  fov: number;
}

/** (sin, cos) of a heading given as an s16 rotation: MK64 forward is (-sin r, cos r) in the port's frame. */
const forwardOf = (rotation: number): [number, number] => {
  const a = -rotation * Math.PI / 32768;
  return [Math.sin(a), Math.cos(a)];
};

export class KartCamera {
  eye: [number, number, number] = [0, 0, 0];
  look: [number, number, number] = [0, 0, 0];
  fov = BASE_FOV;
  /** unk_2C: the camera's own heading (s16). */
  heading = 0;
  /** unk_B0: drift swing (s16). */
  swing = 0;
  /** Mushroom push (unk_DB4.unk0 / unk8) and wall-bump kick (camera unk_94). */
  private pushT = 0; private pushAmp = 0; private kickT = 0; private kickAmp = 0;
  /** D_80164498: field-of-view add. */
  private fovAdd = 0;
  private wasMushroom = false;
  private ready = false;

  /** Put the camera straight behind the kart (camera_init). */
  snap(k: CameraKart): void {
    this.heading = k.rotation;
    this.swing = 0;
    this.pushT = this.pushAmp = this.kickT = this.kickAmp = 0;
    this.fovAdd = 0;
    this.fov = BASE_FOV;
    this.wasMushroom = k.mushroom;
    const [fx, fz] = forwardOf(this.heading);
    const y = k.pos[1] + ORIGIN_LIFT;
    this.eye = [k.pos[0] + fx * CAMERA_BACK, y + CAMERA_HEIGHT, k.pos[2] + fz * CAMERA_BACK];
    this.look = [k.pos[0] + fx * CAMERA_AHEAD, y, k.pos[2] + fz * CAMERA_AHEAD];
    this.ready = true;
  }

  get initialised(): boolean { return this.ready; }

  /** One 60 Hz camera update (func_8001E45C -> func_8001CCEC). */
  step(k: CameraKart): void {
    if (!this.ready) { this.snap(k); return; }
    // Heading follow.
    let step: number;
    if (k.drifting) {
      step = 100;
      if (k.turn === 0) this.swing = 0;
      else if (k.turn < 0) {
        step = 0xa5 - Math.trunc(k.turn / 2);
        this.swing = towardsS16(this.swing, k.driftOutside ? -0xb60 : -12 * DEG, 0.1);
      } else {
        step = Math.trunc(k.turn / 2) + 0xa5;
        this.swing = towardsS16(this.swing, k.driftOutside ? 0xb60 : 12 * DEG, 0.1);
      }
    } else {
      this.swing = towardsS16(this.swing, 0, 0.05);
      const diff = Math.trunc(s16(this.heading) / DEG) - Math.trunc(s16(k.rotation) / DEG);
      if (k.turn === 0) step = 0x1f4;
      else if (k.turn < 0) step = diff <= -70 || diff >= 70 ? 0xb4 - k.turn : 0xa5 - Math.trunc(k.turn / 2);
      else step = diff <= -70 || diff >= 0x46 ? k.turn + 0xb4 : Math.trunc(k.turn / 2) + 0xa5;
    }
    if (!k.spinout) this.heading = adjustAngle(this.heading, s16(k.rotation + this.swing), step);

    // Mushroom push: a decaying series of arcs; a wall hit above 30 km/h kicks the camera up.
    if (k.mushroom && !this.wasMushroom) { this.pushT = 0; this.pushAmp = 8; }
    this.wasMushroom = k.mushroom;
    if (k.bumped && k.kmh > 30) { this.kickT = 0; this.kickAmp = 3; }
    let t = this.pushT + 1, amp = this.pushAmp;
    let push = amp * t - 0.7 * t * t;
    if (push < 0) { t = 0; amp *= 0.8; if (amp <= 0.1) amp = 0; push = 0; }
    this.pushT = t; this.pushAmp = amp;
    let kt = this.kickT + 1, kamp = this.kickAmp;
    let kick = kt * kamp - 1.25 * kt * kt;
    if (kick < 0) { kt = 0; kamp *= 0.9; if (kamp <= 0.1) kamp = 0; kick = 0; }
    this.kickT = kt; this.kickAmp = kamp;

    const [fx, fz] = forwardOf(this.heading);
    const lift = k.pos[1] + ORIGIN_LIFT;
    const eyeBack = CAMERA_BACK + push, lookAhead = CAMERA_AHEAD + push;
    const eyeY = lift + CAMERA_HEIGHT + kick / 2;
    const lookY = lift + kick;
    const slow = k.kmh <= 5 && k.hop;
    this.look[0] += (k.pos[0] + fx * lookAhead - this.look[0]) * FOLLOW_XZ;
    this.look[2] += (k.pos[2] + fz * lookAhead - this.look[2]) * FOLLOW_XZ;
    this.look[1] += (lookY - this.look[1]) * (slow ? 0.02 : FOLLOW_LOOK_Y);
    this.eye[0] += (k.pos[0] + fx * eyeBack - this.eye[0]) * FOLLOW_XZ;
    this.eye[2] += (k.pos[2] + fz * eyeBack - this.eye[2]) * FOLLOW_XZ;
    this.eye[1] += (eyeY - this.eye[1]) * (slow ? 0.01 : FOLLOW_EYE_Y);

    // Field of view: the mushroom opens it up, a spinout more so; 1 degree per frame towards the target.
    if (k.spinout) this.fovAdd = towards(this.fovAdd, SPINOUT_FOV, 0.2);
    else if (k.mushroom) this.fovAdd = towards(this.fovAdd, MUSHROOM_FOV, 0.2);
    else this.fovAdd = this.fovAdd > 1 ? this.fovAdd - 2 : 0;
    const target = BASE_FOV + this.fovAdd;
    if (this.fov < target) this.fov = Math.min(target, this.fov + 1);
    else if (this.fov > target) this.fov = Math.max(target, this.fov - 1);
  }

  pose(): CameraPose { return { eye: [...this.eye], look: [...this.look], fov: this.fov }; }
}

/** Linear blend of two poses (render interpolation between sim ticks). */
export function lerpPose(a: CameraPose, b: CameraPose, t: number): CameraPose {
  const mix = (x: number[], y: number[]) => x.map((v, i) => v + (y[i] - v) * t) as [number, number, number];
  return { eye: mix(a.eye, b.eye), look: mix(a.look, b.look), fov: a.fov + (b.fov - a.fov) * t };
}
