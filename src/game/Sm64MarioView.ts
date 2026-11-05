import * as THREE from 'three';
import type { GLTF } from 'three/addons/loaders/GLTFLoader.js';
import type { CharacterView } from './CharacterView';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { angleToRadians } from '../sim/math';
import { sm64VisualRotation } from './sm64Animation';
import { Sm64AnimationPlayer, type Sm64AnimationMetadata } from './Sm64AnimationPlayer';
import { ACT_IDLE } from '../sim/state';
import { instantiateSm64, loadSm64Asset, SM64_MODELS, Sm64Billboards } from './sm64Assets';

/** Rigid-part SM64 clips already contain pose/flip/root offsets in source units. */
export class Sm64MarioView implements CharacterView {
  readonly root = new THREE.Group();
  readonly clipNames: string[];
  private readonly player: Sm64AnimationPlayer;
  private readonly eyeSwitches: THREE.Object3D[] = [];
  private faceTime = 0;
  private readonly billboards: Sm64Billboards;

  static async load(): Promise<Sm64MarioView> {
    return new Sm64MarioView(await loadSm64Asset(SM64_MODELS.mario));
  }

  private constructor(gltf: GLTF) {
    this.root.name = 'sm64-mario';
    const model = instantiateSm64(gltf);
    this.root.add(model);
    // Wing cap geometry is emitted by the source layout even when the game's
    // callback would hide it. This controller currently has no wing-cap mode.
    model.traverse(object => {
      if (object instanceof THREE.Mesh) object.receiveShadow = false;
      // The native eye callback selects a single head display list (case zero = open).
      if (object.userData.sm64Command === 'GEO_SWITCH_CASE'
          && object.userData.sm64Args?.includes('geo_switch_mario_eyes')) {
        this.eyeSwitches.push(object);
        object.children.forEach((child, index) => { child.visible = index === 0; });
      }
      if (object.userData.sm64Command === 'GEO_DISPLAY_LIST' && object.userData.sm64Args?.[1] === 'mario_cap_wings') object.visible = false;
    });
    this.billboards = new Sm64Billboards(model);
    this.clipNames = gltf.animations.map(clip => clip.name);
    const metadata = new Map<string, Sm64AnimationMetadata>();
    for (const animation of gltf.parser.json.animations ?? []) metadata.set(animation.name, animation.extras ?? {});
    this.player = new Sm64AnimationPlayer(model.children[0], gltf.animations, metadata);
    this.player.update({ action: ACT_IDLE, actionArg: 0, actionState: 0, forwardVel: 0, vel: [0, 0, 0] }, 0);
  }

  setVisible(visible: boolean): void { this.root.visible = visible; }
  get activeClip(): string { return this.player.debug.name; }
  get animationDebug() { return this.player.debug; }

  update(m: MarioRuntime, dt: number, pos: THREE.Vector3, yaw: number, smooth = true): void {
    this.root.position.copy(pos);
    const rotation = sm64VisualRotation(m);
    // Apply local pitch after world yaw, including SM64's graphics-only half-turn.
    this.root.rotation.set(rotation.pitch, angleToRadians(yaw) + rotation.yaw, 0, 'YXZ');
    this.player.update(m, dt, smooth);
    this.faceTime = (this.faceTime + dt) % 3.2;
    const blinkFrame = Math.floor(this.faceTime * 30);
    const eyeCase = blinkFrame === 92 || blinkFrame === 94 ? 1 : blinkFrame === 93 ? 2 : 0;
    for (const node of this.eyeSwitches) {
      const selected = node.children[eyeCase] ? eyeCase : 0;
      node.children.forEach((child, index) => { child.visible = index === selected; });
    }
    this.root.scale.set(m.squishTimer > 0 ? 1.2 : 1, m.squishTimer > 0 ? 0.65 : 1, m.squishTimer > 0 ? 1.2 : 1);
  }

  faceCamera(camera: THREE.Camera): void { this.billboards.update(camera); }
}
