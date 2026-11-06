import type * as THREE from 'three';
import type { CollisionWorld } from '../../sim/collision';
import type { MarioRuntime, SpawnPoint } from '../../sim/MarioRuntime';
import type { Vec3 } from '../../sim/math';
import type { ControllerSnapshot } from '../../sim/state';
import type { CameraRig } from '../CameraRig';
import type { CourseId } from '../courses';
import type { InputManager, PointerHandler } from '../Input';
import type { Particles } from '../Particles';
import type { Sfx } from '../Sfx';
import type { PlaygroundResult } from '../world';

export type ExperimentId = 'dimensions' | 'god' | 'runner' | 'hundred' | 'diorama' | 'race' | 'shrink' | 'planet';
export const EXPERIMENT_IDS: readonly ExperimentId[] = ['dimensions', 'god', 'runner', 'hundred', 'diorama', 'race', 'shrink', 'planet'];
export function isExperimentId(value: unknown): value is ExperimentId {
  return typeof value === 'string' && (EXPERIMENT_IDS as readonly string[]).includes(value);
}

/** The slice of the game shell an experiment may drive. */
export interface ExperimentHost {
  readonly scene: THREE.Scene;
  readonly camera: THREE.PerspectiveCamera;
  readonly canvas: HTMLCanvasElement;
  readonly renderer: THREE.WebGLRenderer;
  readonly input: InputManager;
  readonly particles: Particles;
  readonly sfx: Sfx;
  readonly mario: MarioRuntime;
  readonly playground: PlaygroundResult;
  readonly courseScene: THREE.Scene;
  readonly worldName: CourseId;
  readonly rig: CameraRig;
  /** Interpolated render position of the player. */
  readonly renderPos: THREE.Vector3;
  /** Seconds since the game shell started. */
  readonly time: number;
  /** Root for experiment UI; cleared when the experiment exits. */
  readonly ui: HTMLElement;
  toast(text: string, ms?: number): void;
  /** Teleport the player, reset momentum, and snap the follow camera behind. */
  placeMario(pos: Vec3, faceYaw?: number): void;
  /** Swap the collision world the player (and camera) collide with. */
  setMarioWorld(world: CollisionWorld, spawn: SpawnPoint, deathHeight: number, blockers: THREE.Object3D[]): void;
  /** Return the player and camera to the loaded course's collision. */
  restoreMarioWorld(): void;
  setPlayerVisible(visible: boolean): void;
  /** Move the player, his render interpolation and the follow camera together (no reset). */
  shiftMario(dx: number, dy: number, dz: number): void;
  /** Course coins collected this run, for experiments that award coins. */
  addCoins(count: number): void;
}

export interface StepOverride {
  controller?: ControllerSnapshot;
  cameraYaw?: number;
}

export interface Experiment {
  readonly id: ExperimentId;
  readonly title: string;
  /** Course to load before entering; null keeps whatever course is loaded. */
  readonly course: CourseId | null;
  /** Skip the player's simulation entirely (camera-only experiments). */
  readonly freezesPlayer?: boolean;
  /** Hide the loaded course and skip its objects (the experiment builds its own world). */
  readonly replacesCourse?: boolean;
  /** Hide the course sky dome (the experiment provides its own backdrop). */
  readonly hidesSky?: boolean;
  /** Simulation speed multiplier for the player and world (1 = normal). */
  readonly timeScale?: number;
  readonly pointer?: PointerHandler;
  enter(host: ExperimentHost): void | Promise<void>;
  exit(): void;
  /** Before the player's 30 Hz step. */
  beforeStep?(controller: ControllerSnapshot): StepOverride | void;
  /** After the player's 30 Hz step (or in its place when the player is frozen). */
  afterStep?(): void;
  /** Once per rendered frame, after the default follow camera has been placed.
   * `alpha` is the interpolation factor between the last two 30 Hz steps. */
  frame?(dt: number, alpha: number): void;
  /** Return true when the experiment rendered the frame itself. */
  render?(): boolean;
  keydown?(event: KeyboardEvent): boolean;
  debug(): Record<string, unknown>;
}
