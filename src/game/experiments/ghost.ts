import * as THREE from 'three';
import type { MarioRuntime } from '../../sim/MarioRuntime';
import { Sm64MarioView } from '../Sm64MarioView';

/** One recorded frame: enough of Mario's state to replay his animation. */
export type GhostFrame = [x: number, y: number, z: number, yaw: number, action: number, arg: number, state: number, timer: number, speed: number, vy: number];

export function recordFrame(m: MarioRuntime): GhostFrame {
  return [Math.round(m.pos[0]), Math.round(m.pos[1]), Math.round(m.pos[2]), m.faceAngle[1], m.action, m.actionArg, m.actionState, m.actionTimer, Math.round(m.forwardVel), Math.round(m.vel[1])];
}

/**
 * A translucent Mario that replays a recorded run with his real animations.
 * The view reads a stand-in object shaped like MarioRuntime.
 */
export class GhostMario {
  readonly root: THREE.Group;
  private readonly stand = {
    pos: [0, 0, 0], vel: [0, 0, 0], faceAngle: [0, 0, 0], action: 0, actionArg: 0, actionState: 0, actionTimer: 0,
    forwardVel: 0, flags: 0, squishTimer: 0, ridingShell: null, floor: null, heldObj: null, metalCapTimer: 0,
    vanishCapTimer: 0, wingCapTimer: 0, shellRoll: 0, angleVel: [0, 0, 0], twirlYaw: 0, animFrame: 0, intendedMag: 0,
  };
  private readonly position = new THREE.Vector3();

  private constructor(private readonly view: Sm64MarioView) {
    this.root = view.root;
    view.root.traverse(node => {
      if (!(node instanceof THREE.Mesh)) return;
      node.castShadow = node.receiveShadow = false;
      const material = (node.material as THREE.MeshStandardMaterial).clone();
      material.transparent = true;
      material.opacity = 0.38;
      material.depthWrite = false;
      material.emissive = new THREE.Color(0x6fb7ff);
      material.emissiveIntensity = 0.35;
      node.material = material;
      node.renderOrder = 3;
    });
  }

  static async create(): Promise<GhostMario> {
    return new GhostMario(await Sm64MarioView.load());
  }

  /** Show frame `t` (fractional ticks interpolate position). */
  show(frames: readonly GhostFrame[], t: number, dt: number, camera: THREE.Camera): boolean {
    if (!frames.length || t >= frames.length - 1) { this.root.visible = false; return false; }
    const i = Math.max(0, Math.floor(t)), k = t - i;
    const a = frames[i], b = frames[Math.min(frames.length - 1, i + 1)];
    this.position.set(a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k);
    const s = this.stand;
    s.pos = [this.position.x, this.position.y, this.position.z];
    s.faceAngle = [0, a[3], 0];
    s.action = a[4]; s.actionArg = a[5]; s.actionState = a[6]; s.actionTimer = a[7];
    s.forwardVel = a[8]; s.intendedMag = Math.abs(a[8]); s.vel = [0, a[9], 0];
    this.root.visible = true;
    // The view only reads these fields; it never mutates the stand-in.
    this.view.update(s as unknown as MarioRuntime, dt, this.position, a[3], true);
    this.view.faceCamera(camera);
    return true;
  }

  dispose(): void {
    this.root.removeFromParent();
    this.root.traverse(node => { if (node instanceof THREE.Mesh) (node.material as THREE.Material).dispose(); });
  }
}
