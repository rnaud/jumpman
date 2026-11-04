/**
 * A procedural low-poly Mario, hand-keyed from the action state.
 *
 * SM64's real animation data lives in the ROM, not in the decompilation repo,
 * so this rig is an original set of poses rather than a copy of Nintendo's
 * animation.  It is driven by the same action IDs, timers and velocities that
 * the ported controller uses, which is why it still feels like the right move
 * even though every keyframe here was authored for this project.
 */

import * as THREE from 'three';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { angleToRadians, clamp, lerp } from '../sim/math';
import type { CharacterView } from './CharacterView';
import {
  ACT_AIR_HIT_WALL,
  ACT_BACKFLIP,
  ACT_BACKFLIP_LAND,
  ACT_BACKFLIP_LAND_STOP,
  ACT_BRAKING,
  ACT_BRAKING_STOP,
  ACT_BUTT_SLIDE,
  ACT_BUTT_SLIDE_AIR,
  ACT_BUTT_SLIDE_STOP,
  ACT_CRAWLING,
  ACT_CROUCHING,
  ACT_CROUCH_SLIDE,
  ACT_DECELERATING,
  ACT_DIVE,
  ACT_DIVE_SLIDE,
  ACT_DOUBLE_JUMP,
  ACT_DOUBLE_JUMP_LAND,
  ACT_DOUBLE_JUMP_LAND_STOP,
  ACT_FREEFALL,
  ACT_FREEFALL_LAND,
  ACT_FREEFALL_LAND_STOP,
  ACT_GROUND_POUND,
  ACT_GROUND_POUND_LAND,
  ACT_IDLE,
  ACT_JUMP,
  ACT_JUMP_KICK,
  ACT_JUMP_LAND,
  ACT_JUMP_LAND_STOP,
  ACT_LEDGE_CLIMB_FAST,
  ACT_LEDGE_GRAB,
  ACT_LONG_JUMP,
  ACT_LONG_JUMP_LAND,
  ACT_LONG_JUMP_LAND_STOP,
  ACT_MOVE_PUNCHING,
  ACT_PUNCHING,
  ACT_SIDE_FLIP,
  ACT_SIDE_FLIP_LAND,
  ACT_SIDE_FLIP_LAND_STOP,
  ACT_SLIDE_KICK,
  ACT_SLIDE_KICK_SLIDE,
  ACT_SLIDE_KICK_SLIDE_STOP,
  ACT_SOFT_BONK,
  ACT_START_CRAWLING,
  ACT_START_CROUCHING,
  ACT_STEEP_JUMP,
  ACT_STOMACH_SLIDE,
  ACT_STOP_CRAWLING,
  ACT_STOP_CROUCHING,
  ACT_TRIPLE_JUMP,
  ACT_TRIPLE_JUMP_LAND,
  ACT_TRIPLE_JUMP_LAND_STOP,
  ACT_TURNING_AROUND,
  ACT_WALKING,
  ACT_WALL_KICK_AIR,
} from '../sim/state';

const RED = 0xe33a30;
const BLUE = 0x2b58c8;
const SKIN = 0xf5c49b;
const BROWN = 0x5b2d17;
const WHITE = 0xf7f4ea;
const YELLOW = 0xf3c93b;
const SOLE = 0x2a1710;

const TAU = Math.PI * 2;

function mat(color: number, roughness = 0.75): THREE.MeshStandardMaterial {
  return new THREE.MeshStandardMaterial({ color, roughness, metalness: 0.02, flatShading: true });
}

function box(w: number, h: number, d: number, material: THREE.Material): THREE.Mesh {
  const mesh = new THREE.Mesh(new THREE.BoxGeometry(w, h, d), material);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}

function sphere(r: number, material: THREE.Material, segments = 14): THREE.Mesh {
  const mesh = new THREE.Mesh(
    new THREE.SphereGeometry(r, segments, Math.ceil(segments / 2)),
    material,
  );
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  return mesh;
}

function damp(current: number, target: number, lambda: number, dt: number): number {
  return lerp(current, target, 1 - Math.exp(-lambda * dt));
}

const IDLE_ACTIONS = new Set([
  ACT_IDLE,
  ACT_BRAKING_STOP,
  ACT_BUTT_SLIDE_STOP,
  ACT_SLIDE_KICK_SLIDE_STOP,
  ACT_JUMP_LAND_STOP,
  ACT_DOUBLE_JUMP_LAND_STOP,
  ACT_TRIPLE_JUMP_LAND_STOP,
  ACT_FREEFALL_LAND_STOP,
  ACT_SIDE_FLIP_LAND_STOP,
  ACT_LONG_JUMP_LAND_STOP,
  ACT_BACKFLIP_LAND_STOP,
]);

const LANDING_ACTIONS = new Set([
  ACT_JUMP_LAND,
  ACT_DOUBLE_JUMP_LAND,
  ACT_TRIPLE_JUMP_LAND,
  ACT_FREEFALL_LAND,
  ACT_SIDE_FLIP_LAND,
  ACT_LONG_JUMP_LAND,
  ACT_BACKFLIP_LAND,
]);

const JUMP_FAMILY = new Set([ACT_JUMP, ACT_STEEP_JUMP]);

function makeEmblemTexture(): THREE.CanvasTexture {
  const canvas = document.createElement('canvas');
  canvas.width = 64;
  canvas.height = 64;
  const ctx = canvas.getContext('2d')!;
  ctx.clearRect(0, 0, 64, 64);
  ctx.fillStyle = '#f7f4ea';
  ctx.beginPath();
  ctx.arc(32, 32, 30, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = '#d9342b';
  ctx.font = '900 46px "Arial Black", Arial, sans-serif';
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  ctx.fillText('M', 32, 35);
  const texture = new THREE.CanvasTexture(canvas);
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

export class MarioModel implements CharacterView {
  readonly root = new THREE.Group();

  private readonly flipPivot = new THREE.Group();
  private readonly body = new THREE.Group();
  private readonly torso = new THREE.Group();
  private readonly head = new THREE.Group();
  private readonly eyes = new THREE.Group();
  private readonly cap = new THREE.Group();

  private readonly shoulderL = new THREE.Group();
  private readonly shoulderR = new THREE.Group();
  private readonly elbowL = new THREE.Group();
  private readonly elbowR = new THREE.Group();
  private readonly hipL = new THREE.Group();
  private readonly hipR = new THREE.Group();
  private readonly kneeL = new THREE.Group();
  private readonly kneeR = new THREE.Group();
  private readonly footL = new THREE.Group();
  private readonly footR = new THREE.Group();

  private walkPhase = 0;
  private actionTime = 0;
  private currentAction = -1;
  private blinkTimer = 2.5;
  private blink = 0;

  constructor() {
    const red = mat(RED);
    const blue = mat(BLUE);
    const skin = mat(SKIN, 0.85);
    const brown = mat(BROWN);
    const white = mat(WHITE, 0.6);
    const yellow = mat(YELLOW, 0.4);
    const sole = mat(SOLE, 0.9);

    this.root.add(this.flipPivot);
    this.flipPivot.position.y = 88;
    this.flipPivot.add(this.body);
    this.body.position.y = -88;

    // ---------------------------------------------------------------------
    // Legs: hip -> knee -> foot, with a shoe at the bottom.
    // ---------------------------------------------------------------------
    for (const [hip, knee, foot, dir] of [
      [this.hipL, this.kneeL, this.footL, 1],
      [this.hipR, this.kneeR, this.footR, -1],
    ] as const) {
      hip.position.set(dir * 12, 58, 0);
      hip.rotation.z = dir * -0.02;
      this.body.add(hip);

      const thigh = box(18, 30, 20, blue);
      thigh.position.y = -15;
      hip.add(thigh);

      const kneeJoint = sphere(9.5, blue, 10);
      kneeJoint.position.y = -30;
      hip.add(kneeJoint);

      knee.position.y = -30;
      hip.add(knee);

      const shin = box(16, 22, 18, blue);
      shin.position.y = -11;
      knee.add(shin);

      foot.position.y = -22;
      knee.add(foot);

      const shoe = box(24, 12, 32, brown);
      shoe.position.set(0, 0, 6);
      foot.add(shoe);

      const soleMesh = box(25, 3.5, 33, sole);
      soleMesh.position.set(0, -5.5, 6);
      foot.add(soleMesh);
    }

    // ---------------------------------------------------------------------
    // Torso: red shirt, blue overalls, straps and buttons.
    // ---------------------------------------------------------------------
    this.torso.position.y = 58;
    this.body.add(this.torso);

    const shirt = box(54, 48, 32, red);
    shirt.position.y = 24;
    this.torso.add(shirt);

    const overalls = box(46, 38, 37, blue);
    overalls.position.y = 14;
    this.torso.add(overalls);

    for (const dir of [-1, 1]) {
      const strap = box(8, 28, 4, blue);
      strap.position.set(dir * 14, 38, 18);
      strap.rotation.z = dir * -0.18;
      this.torso.add(strap);

      const button = sphere(3.2, yellow, 10);
      button.position.set(dir * 11, 13, 20);
      this.torso.add(button);
    }

    const collar = box(20, 6, 6, red);
    collar.position.set(0, 48, 15);
    this.torso.add(collar);

    const neck = new THREE.Mesh(new THREE.CylinderGeometry(8, 9, 12, 10), skin);
    neck.position.set(0, 50, 0);
    this.torso.add(neck);

    // ---------------------------------------------------------------------
    // Arms: shoulder -> elbow -> glove.
    // ---------------------------------------------------------------------
    for (const [shoulder, elbow, dir] of [
      [this.shoulderL, this.elbowL, 1],
      [this.shoulderR, this.elbowR, -1],
    ] as const) {
      shoulder.position.set(dir * 34, 42, 0);
      shoulder.rotation.z = dir * 0.1;
      this.torso.add(shoulder);

      const upper = box(14, 26, 15, red);
      upper.position.y = -13;
      shoulder.add(upper);

      const elbowJoint = sphere(7.5, red, 10);
      elbowJoint.position.y = -26;
      shoulder.add(elbowJoint);

      elbow.position.y = -26;
      shoulder.add(elbow);

      const forearm = box(12, 22, 13, red);
      forearm.position.y = -11;
      elbow.add(forearm);

      const cuff = new THREE.Mesh(new THREE.CylinderGeometry(7.5, 7.5, 5, 10), white);
      cuff.position.y = -22;
      elbow.add(cuff);

      const glove = sphere(8.5, white, 12);
      glove.position.y = -26;
      elbow.add(glove);
    }

    // ---------------------------------------------------------------------
    // Head and face.
    // ---------------------------------------------------------------------
    this.head.position.y = 76;
    this.torso.add(this.head);

    const skull = sphere(22.5, skin, 18);
    skull.scale.set(1, 1.06, 0.98);
    this.head.add(skull);

    for (const dir of [-1, 1]) {
      const ear = sphere(4.6, skin, 10);
      ear.position.set(dir * 21.5, 0, -1);
      this.head.add(ear);

      const sideburn = box(4, 10, 5, brown);
      sideburn.position.set(dir * 18.5, -5, 10);
      this.head.add(sideburn);
    }

    const nose = sphere(6.8, skin, 12);
    nose.position.set(0, -4, 20.5);
    nose.scale.set(1.1, 0.95, 1);
    this.head.add(nose);

    const mustache = box(24, 5.5, 6, brown);
    mustache.position.set(0, -10.5, 20);
    mustache.rotation.z = 0.02;
    this.head.add(mustache);

    const hair = sphere(20.5, brown, 14);
    hair.scale.set(1, 0.92, 0.78);
    hair.position.set(0, -2, -9);
    this.head.add(hair);

    this.eyes = new THREE.Group();
    for (const dir of [-1, 1]) {
      const whiteEye = sphere(4.6, white, 10);
      whiteEye.position.set(dir * 7.5, 4, 17.8);
      whiteEye.scale.z = 0.6;
      const pupil = sphere(2.1, mat(0x1a1a1a, 0.4), 8);
      pupil.position.set(dir * 7.5, 4, 20.4);
      pupil.scale.z = 0.5;
      this.eyes.add(whiteEye, pupil);

      const brow = box(7, 2, 2, brown);
      brow.position.set(dir * 8, 9.2, 19.2);
      brow.rotation.z = dir * 0.12;
      this.eyes.add(brow);
    }
    this.head.add(this.eyes);

    // ---------------------------------------------------------------------
    // Cap.
    // ---------------------------------------------------------------------
    this.cap.position.y = 16;
    this.head.add(this.cap);

    const dome = sphere(24, red, 18);
    dome.scale.set(1, 0.58, 1);
    dome.position.y = 3;
    this.cap.add(dome);

    const brim = box(36, 4, 18, red);
    brim.position.set(0, -1, 18);
    brim.rotation.x = 0.04;
    this.cap.add(brim);

    const emblem = new THREE.Mesh(
      new THREE.CircleGeometry(6.5, 20),
      new THREE.MeshBasicMaterial({
        map: makeEmblemTexture(),
        transparent: true,
        depthWrite: false,
      }),
    );
    emblem.position.set(0, 7, 21.7);
    emblem.castShadow = false;
    this.cap.add(emblem);

    this.root.traverse((object) => {
      if (object instanceof THREE.Mesh) {
        object.castShadow = true;
        object.receiveShadow = true;
      }
    });
  }

  setVisible(visible: boolean): void {
    this.root.visible = visible;
  }

  get poseDebug(): Record<string, number> {
    return {
      walkPhase: this.walkPhase,
      hipL: this.hipL.rotation.x,
      hipR: this.hipR.rotation.x,
      kneeL: this.kneeL.rotation.x,
      shLX: this.shoulderL.rotation.x,
      shRX: this.shoulderR.rotation.x,
      bodyY: this.body.position.y,
    };
  }

  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number): void {
    this.root.position.copy(pos);
    this.root.rotation.y = angleToRadians(yaw);

    if (m.action !== this.currentAction) {
      this.currentAction = m.action;
      this.actionTime = 0;
    } else {
      this.actionTime += dt;
    }

    const action = m.action;
    const speed = Math.abs(m.forwardVel);

    // Defaults -------------------------------------------------------------
    let hipLX = 0;
    let hipRX = 0;
    let kneeLX = 0.12;
    let kneeRX = 0.12;
    let footLX = -0.06;
    let footRX = -0.06;

    let shLX = 0;
    let shRX = 0;
    let shLZ = 0.12;
    let shRZ = -0.12;
    let elLX = -0.16;
    let elRX = -0.16;

    let headX = 0;
    let headY = 0;
    let torsoX = 0;
    let torsoY = 0;
    let bodyY = 0;
    let leanX = 0;
    let leanZ = 0;
    let scaleX = 1;
    let scaleY = 1;
    let scaleZ = 1;
    let flipX: number | null = null;
    let flipZ: number | null = null;
    let spinY = 0;

    // Idle breathing / blinking
    this.blinkTimer -= dt;
    if (this.blinkTimer <= 0) {
      this.blinkTimer = 2.4 + Math.random() * 3.4;
      this.blink = 1;
    }
    this.blink = Math.max(0, this.blink - dt * 9);

    const idleBreath = Math.sin(this.actionTime * 2.1);

    if (IDLE_ACTIONS.has(action)) {
      bodyY = idleBreath * 0.9;
      torsoX = 0.02 + idleBreath * 0.008;
      headY = Math.sin(this.actionTime * 0.55) * 0.12;
      headX = Math.sin(this.actionTime * 0.37) * 0.04;
      shLX = idleBreath * 0.02;
      shRX = -shLX;
      kneeLX = 0.1;
      kneeRX = 0.1;
    } else if (LANDING_ACTIONS.has(action)) {
      const squash = clamp(1 - this.actionTime / 0.2, 0, 1);
      scaleY = 1 - 0.24 * squash;
      scaleX = 1 + 0.13 * squash;
      scaleZ = 1 + 0.13 * squash;
      bodyY = -16 * squash;
      hipLX = -0.45 * squash;
      hipRX = -0.45 * squash;
      kneeLX = 0.3 + 0.95 * squash;
      kneeRX = 0.3 + 0.95 * squash;
      footLX = -0.3 * squash;
      footRX = -0.3 * squash;
      shLX = -0.4 * squash;
      shRX = -0.4 * squash;
      shLZ = 0.45;
      shRZ = -0.45;
      headX = 0.12 * squash;
    } else if (
      action === ACT_WALKING ||
      action === ACT_DECELERATING ||
      action === ACT_TURNING_AROUND
    ) {
      const runSpeed = Math.min(speed, 48);
      const rate = Math.max(2.2, runSpeed * 0.3);
      this.walkPhase += dt * rate;

      const stride = Math.sin(this.walkPhase);
      const amp = 0.22 + (runSpeed / 48) * 0.72;
      const bend = 0.45 + (runSpeed / 48) * 0.85;

      hipLX = stride * amp;
      hipRX = -hipLX;
      kneeLX = 0.12 + Math.max(0, stride) * bend;
      kneeRX = 0.12 + Math.max(0, -stride) * bend;
      footLX = -(hipLX + kneeLX) * 0.42;
      footRX = -(hipRX + kneeRX) * 0.42;

      shLX = -hipLX * 0.95;
      shRX = -hipRX * 0.95;
      shLZ = 0.12;
      shRZ = -0.12;
      elLX = -0.24 - Math.abs(hipLX) * 0.6;
      elRX = -0.24 - Math.abs(hipRX) * 0.6;

      bodyY = Math.abs(Math.sin(this.walkPhase)) * (1.2 + runSpeed * 0.07);
      leanX = 0.02 + (runSpeed / 48) * 0.16;
      torsoX = 0.02 + (runSpeed / 48) * 0.05;
      torsoY = -stride * (0.04 + (runSpeed / 48) * 0.07);
      headY = -torsoY * 0.7;
      headX = -leanX * 0.35;
      kneeLX += 0.03;
      kneeRX += 0.03;

      if (action === ACT_DECELERATING && speed > 8) {
        leanX -= 0.16;
        headX += 0.08;
      }
    } else if (action === ACT_BRAKING) {
      hipLX = -0.75;
      hipRX = 0.55;
      kneeLX = 0.45;
      kneeRX = 0.55;
      footLX = 0.1;
      footRX = -0.35;
      shLX = -0.5;
      shRX = -0.5;
      shLZ = 1.15;
      shRZ = -1.15;
      leanX = -0.28;
      torsoX = -0.08;
      headX = -0.16;
      bodyY = -8;
    } else if (
      action === ACT_CROUCHING ||
      action === ACT_START_CROUCHING ||
      action === ACT_STOP_CROUCHING
    ) {
      hipLX = -0.85;
      hipRX = -0.85;
      kneeLX = 1.45;
      kneeRX = 1.45;
      footLX = -0.55;
      footRX = -0.55;
      shLX = -0.55;
      shRX = -0.55;
      shLZ = 0.42;
      shRZ = -0.42;
      elLX = -0.5;
      elRX = -0.5;
      bodyY = -18;
      torsoX = 0.14;
      scaleY = 0.94;
      headX = -0.1;
    } else if (
      action === ACT_CRAWLING ||
      action === ACT_START_CRAWLING ||
      action === ACT_STOP_CRAWLING
    ) {
      const crawlPhase = Math.sin(this.actionTime * 9);
      hipLX = -1.15 + crawlPhase * 0.3;
      hipRX = -1.15 - crawlPhase * 0.3;
      kneeLX = 1.75;
      kneeRX = 1.75;
      footLX = -0.5;
      footRX = -0.5;
      shLX = -1.2 + crawlPhase * 0.45;
      shRX = -1.2 - crawlPhase * 0.45;
      shLZ = 0.5;
      shRZ = -0.5;
      elLX = -0.5;
      elRX = -0.5;
      bodyY = -30;
      torsoX = 0.38;
      scaleY = 0.9;
      headX = -0.3;
    } else if (action === ACT_CROUCH_SLIDE) {
      hipLX = -1.3;
      hipRX = -0.6;
      kneeLX = 1.0;
      kneeRX = 1.5;
      footLX = -0.5;
      footRX = -0.6;
      shLX = 0.4;
      shRX = -0.7;
      shLZ = 0.7;
      shRZ = -0.5;
      bodyY = -28;
      leanX = -0.35;
      torsoX = -0.1;
      scaleY = 0.9;
      headX = -0.15;
    } else if (JUMP_FAMILY.has(action)) {
      const takeoff = clamp(1 - this.actionTime / 0.13, 0, 1);
      const air = clamp(1 - takeoff, 0, 1);
      const rise = clamp(m.vel[1] / 40, 0, 1);
      const fall = clamp(-m.vel[1] / 40, 0, 1);

      hipLX = -0.35 * air + 0.25 * fall + 0.55 * takeoff;
      hipRX = hipLX;
      kneeLX = 0.15 + 0.75 * air - 0.1 * fall + 1.0 * takeoff;
      kneeRX = kneeLX;
      footLX = -0.3 * air - 0.2 * takeoff;
      footRX = footLX;

      shLX = -1.4 * air - 0.65 * rise + 0.9 * fall + 0.7 * takeoff;
      shRX = shLX;
      shLZ = 0.62 + 0.28 * air;
      shRZ = -shLZ;
      elLX = -0.25 - 0.4 * air;
      elRX = elLX;

      leanX = 0.08 * air + 0.04 * takeoff;
      torsoX = 0.02;
      headX = -leanX * 0.4;
      bodyY = -8 * takeoff - 4 * air;

      if (action === ACT_STEEP_JUMP) {
        leanX = 0.22;
        hipLX = -0.6;
        hipRX = -0.2;
      }
    } else if (action === ACT_DOUBLE_JUMP) {
      const p = clamp(this.actionTime / 0.62, 0, 1);
      flipX = p * TAU;
      hipLX = -0.7;
      hipRX = -0.55;
      kneeLX = 1.3;
      kneeRX = 1.15;
      footLX = -0.45;
      footRX = -0.4;
      shLX = -1.15;
      shRX = -1.15;
      shLZ = 0.45;
      shRZ = -0.45;
      elLX = -1.1;
      elRX = -1.1;
      bodyY = -5;
    } else if (action === ACT_TRIPLE_JUMP) {
      const p = clamp(this.actionTime / 0.78, 0, 1);
      flipX = p * TAU;
      hipLX = -0.4;
      hipRX = 0.3;
      kneeLX = 0.85;
      kneeRX = 0.4;
      shLX = -1.5;
      shRX = -1.5;
      shLZ = 1.25;
      shRZ = -1.25;
      elLX = -0.35;
      elRX = -0.35;
      bodyY = -4;
    } else if (action === ACT_BACKFLIP) {
      const p = clamp(this.actionTime / 0.7, 0, 1);
      flipX = -p * TAU;
      hipLX = -0.8;
      hipRX = -0.55;
      kneeLX = 1.2;
      kneeRX = 1.0;
      shLX = -1.7;
      shRX = -1.7;
      shLZ = 1.0;
      shRZ = -1.0;
      elLX = -0.6;
      elRX = -0.6;
      bodyY = -5;
    } else if (action === ACT_SIDE_FLIP) {
      const p = clamp(this.actionTime / 0.7, 0, 1);
      flipZ = p * TAU;
      hipLX = -0.8;
      hipRX = 0.55;
      kneeLX = 1.0;
      kneeRX = 0.5;
      shLX = -1.0;
      shRX = -1.0;
      shLZ = 1.8;
      shRZ = -1.8;
      elLX = -0.3;
      elRX = -0.3;
      bodyY = -5;
    } else if (action === ACT_LONG_JUMP) {
      leanX = 0.48;
      hipLX = 0.55;
      hipRX = 0.4;
      kneeLX = 0.3;
      kneeRX = 0.2;
      footLX = 0.15;
      footRX = 0.1;
      shLX = 0.9;
      shRX = 0.9;
      shLZ = 0.55;
      shRZ = -0.55;
      elLX = -0.25;
      elRX = -0.25;
      bodyY = -4;
      headX = -0.3;
    } else if (action === ACT_DIVE) {
      leanX = 1.22;
      hipLX = 0.2;
      hipRX = 0.2;
      kneeLX = 0.15;
      kneeRX = 0.15;
      footLX = 0.1;
      footRX = 0.1;
      shLX = -1.9;
      shRX = -1.9;
      shLZ = 0.38;
      shRZ = -0.38;
      elLX = -0.1;
      elRX = -0.1;
      headX = -0.55;
    } else if (action === ACT_JUMP_KICK) {
      leanX = 0.12;
      hipLX = -1.25;
      hipRX = 0.35;
      kneeLX = 0.2;
      kneeRX = 0.7;
      shLX = -1.0;
      shRX = -1.0;
      shLZ = 1.0;
      shRZ = -1.0;
    } else if (action === ACT_WALL_KICK_AIR) {
      leanX = -0.08;
      hipLX = -1.35;
      hipRX = 0.35;
      kneeLX = 0.15;
      kneeRX = 0.55;
      shLX = -0.6;
      shRX = -0.6;
      shLZ = 1.3;
      shRZ = -1.3;
      bodyY = -3;
    } else if (action === ACT_FREEFALL || action === ACT_SOFT_BONK || action === ACT_AIR_HIT_WALL) {
      const flail = Math.sin(this.actionTime * 11) * 0.25;
      hipLX = 0.15 + flail;
      hipRX = 0.15 - flail;
      kneeLX = 0.35;
      kneeRX = 0.35;
      shLX = -0.7 + flail;
      shRX = -0.7 - flail;
      shLZ = 1.15;
      shRZ = -1.15;
      elLX = -0.4;
      elRX = -0.4;
      leanX = action === ACT_SOFT_BONK ? -0.2 : 0.05;
      headX = -leanX * 0.4;
    } else if (action === ACT_GROUND_POUND) {
      if (m.actionState === 0) {
        spinY = this.actionTime * 13;
        hipLX = -0.7;
        hipRX = -0.7;
        kneeLX = 1.35;
        kneeRX = 1.35;
        shLX = -1.0;
        shRX = -1.0;
        shLZ = 1.45;
        shRZ = -1.45;
        bodyY = -10;
      } else {
        hipLX = -0.1;
        hipRX = -0.1;
        kneeLX = 0.15;
        kneeRX = 0.15;
        shLX = -0.2;
        shRX = -0.2;
        shLZ = 0.2;
        shRZ = -0.2;
        headX = 0.2;
      }
    } else if (action === ACT_GROUND_POUND_LAND) {
      scaleY = 0.7;
      scaleX = 1.28;
      scaleZ = 1.28;
      bodyY = -14;
      hipLX = -0.8;
      hipRX = -0.8;
      kneeLX = 1.4;
      kneeRX = 1.4;
      shLX = -0.1;
      shRX = -0.1;
      shLZ = 1.1;
      shRZ = -1.1;
    } else if (
      action === ACT_BUTT_SLIDE ||
      action === ACT_BUTT_SLIDE_AIR ||
      action === ACT_BUTT_SLIDE_STOP
    ) {
      leanX = -0.5;
      bodyY = -28;
      hipLX = -1.35;
      hipRX = -1.15;
      kneeLX = 0.3;
      kneeRX = 0.2;
      footLX = 0.25;
      footRX = 0.2;
      shLX = 0.55;
      shRX = 0.4;
      shLZ = 0.75;
      shRZ = -0.75;
      headX = -0.3;
      torsoX = -0.08;
    } else if (action === ACT_STOMACH_SLIDE || action === ACT_DIVE_SLIDE) {
      leanX = 1.3;
      bodyY = -32;
      hipLX = 0.25;
      hipRX = -0.15;
      kneeLX = 0.15;
      kneeRX = 0.2;
      shLX = -2.0;
      shRX = -2.0;
      shLZ = 0.45;
      shRZ = -0.45;
      elLX = -0.15;
      elRX = -0.15;
      headX = -0.5;
    } else if (
      action === ACT_SLIDE_KICK ||
      action === ACT_SLIDE_KICK_SLIDE ||
      action === ACT_SLIDE_KICK_SLIDE_STOP
    ) {
      leanX = -0.3;
      bodyY = -20;
      hipLX = -1.45;
      hipRX = 0.2;
      kneeLX = 0.15;
      kneeRX = 0.55;
      shLX = 0.8;
      shRX = -0.5;
      shLZ = 0.5;
      shRZ = -0.6;
      headX = -0.2;
    } else if (action === ACT_PUNCHING || action === ACT_MOVE_PUNCHING) {
      const extension = clamp(m.actionTimer / 4, 0, 1);
      if (m.actionArg === 2 || m.actionArg === 6) {
        hipLX = -1.25;
        hipRX = 0.25;
        kneeLX = 0.2;
        kneeRX = 0.55;
        shLX = 0.4;
        shRX = -0.9;
        shLZ = 0.9;
        shRZ = -0.6;
        torsoY = -0.2;
      } else if (m.actionArg === 1 || m.actionArg === 4) {
        shRX = -1.55 * extension - 0.2;
        elRX = -0.15;
        shLX = 0.35;
        elLX = -0.6;
        torsoY = -0.42 * extension;
        headY = 0.15;
      } else {
        shLX = -1.55 * extension - 0.2;
        elLX = -0.15;
        shRX = 0.35;
        elRX = -0.6;
        torsoY = 0.42 * extension;
        headY = -0.15;
      }
      hipLX = -0.15;
      hipRX = 0.15;
      kneeLX = 0.25;
      kneeRX = 0.25;
      bodyY = -2;
    } else if (action === ACT_LEDGE_GRAB || action === ACT_LEDGE_CLIMB_FAST) {
      const climb = action === ACT_LEDGE_CLIMB_FAST ? clamp(this.actionTime / 0.5, 0, 1) : 0;
      shLX = -2.55 + climb * 0.4;
      shRX = -2.55 + climb * 0.4;
      shLZ = 0.22;
      shRZ = -0.22;
      elLX = -0.2;
      elRX = -0.2;
      hipLX = 0.25 - climb * 0.7;
      hipRX = 0.1 - climb * 0.4;
      kneeLX = 0.4 + climb * 0.4;
      kneeRX = 0.25 + climb * 0.3;
      bodyY = idleBreath * 0.4;
      headX = -0.2;
    }

    if (m.squishTimer > 0) {
      scaleY = 0.55;
      scaleX = 1.32;
      scaleZ = 1.32;
      bodyY = -14;
    }

    // Apply with damping ---------------------------------------------------
    const limbK = 1 - Math.exp(-24 * dt);
    const bodyK = 1 - Math.exp(-15 * dt);
    const scaleK = 1 - Math.exp(-20 * dt);

    this.hipL.rotation.x = lerp(this.hipL.rotation.x, hipLX, limbK);
    this.hipR.rotation.x = lerp(this.hipR.rotation.x, hipRX, limbK);
    this.kneeL.rotation.x = lerp(this.kneeL.rotation.x, kneeLX, limbK);
    this.kneeR.rotation.x = lerp(this.kneeR.rotation.x, kneeRX, limbK);
    this.footL.rotation.x = lerp(this.footL.rotation.x, footLX, limbK);
    this.footR.rotation.x = lerp(this.footR.rotation.x, footRX, limbK);

    this.shoulderL.rotation.x = lerp(this.shoulderL.rotation.x, shLX, limbK);
    this.shoulderR.rotation.x = lerp(this.shoulderR.rotation.x, shRX, limbK);
    this.shoulderL.rotation.z = lerp(this.shoulderL.rotation.z, shLZ, limbK);
    this.shoulderR.rotation.z = lerp(this.shoulderR.rotation.z, shRZ, limbK);
    this.elbowL.rotation.x = lerp(this.elbowL.rotation.x, elLX, limbK);
    this.elbowR.rotation.x = lerp(this.elbowR.rotation.x, elRX, limbK);

    this.torso.rotation.x = lerp(this.torso.rotation.x, torsoX, bodyK);
    this.torso.rotation.y = lerp(this.torso.rotation.y, torsoY, bodyK);
    this.head.rotation.x = lerp(this.head.rotation.x, headX, bodyK);
    this.head.rotation.y = lerp(this.head.rotation.y, headY - torsoY * 0.5, bodyK);

    this.body.position.y = lerp(this.body.position.y, -88 + bodyY, bodyK);
    this.body.rotation.x = lerp(this.body.rotation.x, leanX, bodyK);
    this.body.rotation.z = lerp(this.body.rotation.z, leanZ, bodyK);
    this.body.scale.x = lerp(this.body.scale.x, scaleX, scaleK);
    this.body.scale.y = lerp(this.body.scale.y, scaleY, scaleK);
    this.body.scale.z = lerp(this.body.scale.z, scaleZ, scaleK);

    if (flipX !== null) {
      this.flipPivot.rotation.x = flipX;
    } else {
      this.flipPivot.rotation.x = damp(this.flipPivot.rotation.x, 0, 18, dt);
    }
    if (flipZ !== null) {
      this.flipPivot.rotation.z = flipZ;
    } else {
      this.flipPivot.rotation.z = damp(this.flipPivot.rotation.z, 0, 18, dt);
    }
    this.flipPivot.rotation.y = spinY;

    this.cap.rotation.x = lerp(this.cap.rotation.x, -leanX * 0.12 + headX * 0.25, bodyK);
    this.cap.rotation.z = lerp(
      this.cap.rotation.z,
      -leanZ * 0.2 + Math.sin(this.walkPhase) * 0.012,
      bodyK,
    );

    this.eyes.scale.y = 1 - this.blink * 0.9;
  }
}
