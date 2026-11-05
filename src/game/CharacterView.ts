import type * as THREE from 'three';
import type { MarioRuntime } from '../sim/MarioRuntime';

/**
 * A renderable character driven by the Mario simulation.  Both the built-in
 * procedural Mario rig and the CC0 glTF rig implement this interface, so the
 * game can swap them without touching physics.
 */
export interface CharacterView {
  readonly root: THREE.Object3D;
  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number, smooth?: boolean): void;
  setVisible(visible: boolean): void;
  faceCamera?(camera: THREE.Camera): void;
}
