/**
 * A glTF character driven by SM64 action IDs.
 *
 * The bundled CC0 rig is Quaternius' Universal Animation Library: a mannequin
 * mesh plus 46 humanoid clips.  The same class loads any glTF with compatible
 * clip names, which is the "bring your own rig" path as well.
 */

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import type { MarioRuntime } from '../sim/MarioRuntime';
import { angleToRadians, clamp, lerp } from '../sim/math';
import type { CharacterView } from './CharacterView';
import {
  mergeAliases,
  resolveClipName,
  type ClipAliasKey,
  type ClipAliases,
} from './clipMap';
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

const FLIP_ACTIONS = new Set([
  ACT_DOUBLE_JUMP,
  ACT_TRIPLE_JUMP,
  ACT_BACKFLIP,
  ACT_SIDE_FLIP,
  ACT_JUMP_KICK,
  ACT_SLIDE_KICK,
]);

interface ClipChoice {
  key: ClipAliasKey;
  loop: boolean;
  timeScale: number;
  fade: number;
}

export class GltfCharacterView implements CharacterView {
  readonly root = new THREE.Group();
  readonly clipNames: string[];
  private readonly aliases: ClipAliases;

  private readonly visual = new THREE.Group();
  private readonly model: THREE.Object3D;
  private readonly mixer: THREE.AnimationMixer;
  private readonly actions = new Map<string, THREE.AnimationAction>();
  private current: THREE.AnimationAction | null = null;
  private currentName = '';

  private actionTime = 0;
  private currentAction = -1;
  private tiltX = 0;
  private tiltZ = 0;
  private spinY = 0;
  private scaleX = 1;
  private scaleY = 1;
  private scaleZ = 1;

  static async load(
    url: string,
    options: { aliases?: unknown } = {},
  ): Promise<GltfCharacterView> {
    const loader = new GLTFLoader();
    const gltf = await loader.loadAsync(url);
    return new GltfCharacterView(gltf, mergeAliases(options.aliases));
  }

  private constructor(
    gltf: { scene: THREE.Group; animations: THREE.AnimationClip[] },
    aliases: ClipAliases,
  ) {
    this.aliases = aliases;
    this.root.add(this.visual);
    this.model = gltf.scene;
    this.visual.add(this.model);

    // The author's mannequin is authored in metres.  Scale it to SM64's
    // 160-unit Mario height and put its feet on the root origin.
    this.model.updateMatrixWorld(true);
    const bounds = new THREE.Box3().setFromObject(this.model);
    const height = Math.max(0.001, bounds.max.y - bounds.min.y);
    const scale = 160 / height;
    this.model.scale.setScalar(scale);
    this.model.updateMatrixWorld(true);

    const scaled = new THREE.Box3().setFromObject(this.model);
    this.model.position.x -= (scaled.min.x + scaled.max.x) / 2;
    this.model.position.z -= (scaled.min.z + scaled.max.z) / 2;
    this.model.position.y -= scaled.min.y;

    this.model.traverse((object) => {
      if (!(object instanceof THREE.Mesh)) return;
      object.castShadow = true;
      object.receiveShadow = true;
      object.frustumCulled = false;

      const materials = Array.isArray(object.material) ? object.material : [object.material];
      object.material = materials.map((material) => {
        const clone = material.clone() as THREE.MeshStandardMaterial;
        if (clone.name === 'M_Main') {
          clone.color.setHex(0xd94032); // red body
        } else if (clone.name === 'M_Joints') {
          clone.color.setHex(0x315bd6); // blue joints
        }
        clone.roughness = 0.72;
        clone.metalness = 0.02;
        return clone;
      });
      if (object.material.length === 1) object.material = object.material[0];
    });

    this.mixer = new THREE.AnimationMixer(this.model);
    this.clipNames = gltf.animations.map((clip) => clip.name);

    for (const clip of gltf.animations) {
      this.actions.set(clip.name, this.mixer.clipAction(clip));
    }

    this.play('idle', true, 1, 0);
  }

  setVisible(visible: boolean): void {
    this.root.visible = visible;
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

    const choice = this.chooseClip(m);
    this.play(choice.key, choice.loop, choice.timeScale, choice.fade);

    // A few actions need a visual adjustment on top of the clip.
    let tiltX = 0;
    let tiltZ = 0;
    let spinY = 0;
    let scaleX = 1;
    let scaleY = 1;
    let scaleZ = 1;

    if (m.action === ACT_DIVE) tiltX = 1.1;
    else if (m.action === ACT_LONG_JUMP) tiltX = 0.38;
    else if (m.action === ACT_STOMACH_SLIDE || m.action === ACT_DIVE_SLIDE) tiltX = 1.18;
    else if (
      m.action === ACT_BUTT_SLIDE ||
      m.action === ACT_BUTT_SLIDE_AIR ||
      m.action === ACT_BUTT_SLIDE_STOP
    ) {
      tiltX = -0.38;
    } else if (m.action === ACT_SLIDE_KICK_SLIDE) {
      tiltX = -0.28;
    } else if (m.action === ACT_GROUND_POUND) {
      spinY = this.actionTime * 12;
      scaleY = 0.96;
    } else if (m.action === ACT_GROUND_POUND_LAND) {
      scaleY = 0.78;
      scaleX = 1.12;
      scaleZ = 1.12;
    } else if (m.action === ACT_BRAKING) {
      tiltX = -0.2;
    } else if (m.action === ACT_SIDE_FLIP) {
      tiltZ = 0;
    }

    if (m.squishTimer > 0) {
      scaleY = 0.62;
      scaleX = 1.22;
      scaleZ = 1.22;
    }

    const damp = 1 - Math.exp(-16 * dt);
    this.tiltX = lerp(this.tiltX, tiltX, damp);
    this.tiltZ = lerp(this.tiltZ, tiltZ, damp);
    this.spinY = lerp(this.spinY, spinY, damp);
    this.scaleX = lerp(this.scaleX, scaleX, damp);
    this.scaleY = lerp(this.scaleY, scaleY, damp);
    this.scaleZ = lerp(this.scaleZ, scaleZ, damp);

    this.visual.rotation.x = this.tiltX;
    this.visual.rotation.z = this.tiltZ;
    this.visual.rotation.y = this.spinY;
    this.visual.scale.set(this.scaleX, this.scaleY, this.scaleZ);

    this.mixer.update(dt);
  }

  private chooseClip(m: MarioRuntime): ClipChoice {
    const speed = Math.abs(m.forwardVel);

    if (IDLE_ACTIONS.has(m.action)) {
      return this.loop('idle', 1);
    }

    if (LANDING_ACTIONS.has(m.action)) {
      return this.once('jumpLand', 1);
    }

    if (m.action === ACT_WALKING) {
      if (speed > 27) return this.loop('sprint', clamp(speed / 32, 0.8, 1.5));
      if (speed > 10) return this.loop('jog', clamp(speed / 22, 0.7, 1.35));
      return this.loop('walk', clamp(speed / 12, 0.65, 1.1));
    }

    if (m.action === ACT_DECELERATING) {
      return this.loop(speed > 10 ? 'jog' : 'walk', 0.7);
    }

    if (m.action === ACT_TURNING_AROUND) {
      return this.loop('walk', 0.65);
    }

    if (m.action === ACT_BRAKING) {
      return this.loop('walk', 0.35);
    }

    if (
      m.action === ACT_CROUCHING ||
      m.action === ACT_START_CROUCHING ||
      m.action === ACT_STOP_CROUCHING
    ) {
      return this.loop('crouchIdle', 1);
    }

    if (
      m.action === ACT_CRAWLING ||
      m.action === ACT_START_CRAWLING ||
      m.action === ACT_STOP_CRAWLING
    ) {
      return this.loop('crouchWalk', 0.8);
    }

    if (m.action === ACT_CROUCH_SLIDE) {
      return this.loop('crouchWalk', 1.25);
    }

    if (
      m.action === ACT_JUMP ||
      m.action === ACT_STEEP_JUMP ||
      m.action === ACT_WALL_KICK_AIR
    ) {
      if (this.actionTime < 0.28) return this.once('jumpStart', 1.25);
      return this.loop('jumpLoop', 1);
    }

    if (m.action === ACT_FREEFALL || m.action === ACT_SOFT_BONK || m.action === ACT_AIR_HIT_WALL) {
      return this.loop('jumpLoop', 0.95);
    }

    if (m.action === ACT_DOUBLE_JUMP) {
      return this.once('roll', 1.25);
    }

    if (m.action === ACT_TRIPLE_JUMP || m.action === ACT_BACKFLIP) {
      return this.once('roll', 1.45);
    }

    if (m.action === ACT_SIDE_FLIP) {
      return this.once('rollRM', 1.4);
    }

    if (m.action === ACT_LONG_JUMP) {
      return this.loop('jumpLoop', 1.05);
    }

    if (m.action === ACT_DIVE) {
      return this.loop('jumpLoop', 0.85);
    }

    if (FLIP_ACTIONS.has(m.action)) {
      return this.once('roll', 1.15);
    }

    if (
      m.action === ACT_BUTT_SLIDE ||
      m.action === ACT_BUTT_SLIDE_AIR ||
      m.action === ACT_BUTT_SLIDE_STOP ||
      m.action === ACT_STOMACH_SLIDE ||
      m.action === ACT_DIVE_SLIDE ||
      m.action === ACT_SLIDE_KICK_SLIDE
    ) {
      return this.loop('roll', 1.2);
    }

    if (m.action === ACT_SLIDE_KICK_SLIDE_STOP) {
      return this.once('crouchIdle', 1);
    }

    if (m.action === ACT_GROUND_POUND) {
      return this.once('roll', 1.35);
    }

    if (m.action === ACT_GROUND_POUND_LAND) {
      return this.once('jumpLand', 1);
    }

    if (m.action === ACT_PUNCHING || m.action === ACT_MOVE_PUNCHING) {
      if (m.actionArg === 1 || m.actionArg === 4) {
        return this.once('punchCross', 1.4);
      }
      return this.once('punchJab', 1.4);
    }

    if (m.action === ACT_LEDGE_GRAB || m.action === ACT_LEDGE_CLIMB_FAST) {
      return this.once('interact', 1);
    }

    return this.loop('idle', 1);
  }

  private loop(key: ClipAliasKey, timeScale: number): ClipChoice {
    return { key, loop: true, timeScale, fade: 0.14 };
  }

  private once(key: ClipAliasKey, timeScale: number): ClipChoice {
    return { key, loop: false, timeScale, fade: 0.1 };
  }

  private play(key: ClipAliasKey, loop: boolean, timeScale: number, fade: number): void {
    const name = resolveClipName(this.clipNames, this.aliases, key);
    if (!name) return;

    const action = this.actions.get(name);
    if (!action) return;

    action.timeScale = timeScale;

    if (action === this.current) {
      action.setLoop(loop ? THREE.LoopRepeat : THREE.LoopOnce, loop ? Infinity : 1);
      action.clampWhenFinished = !loop;
      return;
    }

    if (this.current) {
      this.current.fadeOut(fade);
    }

    action
      .reset()
      .setEffectiveWeight(1)
      .setEffectiveTimeScale(timeScale)
      .setLoop(loop ? THREE.LoopRepeat : THREE.LoopOnce, loop ? Infinity : 1);
    action.clampWhenFinished = !loop;
    action.fadeIn(fade).play();

    this.current = action;
    this.currentName = name;
  }

  get activeClip(): string {
    return this.currentName;
  }
}
